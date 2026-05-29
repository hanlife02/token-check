use crate::model::{ReportData, SessionMeta, Source, Usage, UsageEvent};
use anyhow::{Context, Result};
use chrono::{DateTime, Local};
use rusqlite::Connection;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

#[derive(Clone, Debug)]
struct SessionInfo {
    date: String,
    project: String,
    model: String,
}

#[derive(Clone, Debug)]
struct MessageRow {
    id: String,
    session_id: String,
    time_created: i64,
    time_updated: i64,
    data: String,
    session_directory: Option<String>,
    session_time_created: Option<i64>,
}

pub fn collect(opencode_root: &Path) -> Result<ReportData> {
    let mut data = ReportData::default();
    if !opencode_root.exists() {
        data.warnings.push(format!(
            "OpenCode data directory not found: {}",
            opencode_root.display()
        ));
        return Ok(data);
    }

    let mut sessions = BTreeMap::new();
    let mut usage_events = Vec::new();
    let mut seen_event_ids = BTreeSet::new();

    collect_session_files(
        &opencode_root.join("storage").join("session"),
        &mut sessions,
        &mut data.warnings,
    );

    let db_path = opencode_root.join("opencode.db");
    if db_path.exists() {
        if let Err(err) = collect_db(
            &db_path,
            &mut sessions,
            &mut usage_events,
            &mut seen_event_ids,
        ) {
            data.warnings.push(format!(
                "Failed to read OpenCode database {}: {err:#}",
                db_path.display()
            ));
        }
    }

    let message_root = opencode_root.join("storage").join("message");
    if message_root.exists() {
        collect_message_files(
            &message_root,
            &mut sessions,
            &mut usage_events,
            &mut seen_event_ids,
            &mut data.warnings,
        );
    } else if !db_path.exists() {
        data.warnings.push(format!(
            "OpenCode data directory not found: {}",
            message_root.display()
        ));
    }

    let event_sessions = usage_events
        .iter()
        .map(|event| event.session_id.clone())
        .collect::<BTreeSet<_>>();
    data.sessions = sessions
        .into_iter()
        .filter(|(session_id, _)| event_sessions.contains(session_id))
        .map(|(session_id, info)| SessionMeta {
            source: Source::OpenCode,
            session_id,
            date: info.date,
            project: info.project,
            model: info.model,
        })
        .collect();
    data.usage_events = usage_events;
    Ok(data)
}

fn collect_db(
    db_path: &Path,
    sessions: &mut BTreeMap<String, SessionInfo>,
    usage_events: &mut Vec<UsageEvent>,
    seen_event_ids: &mut BTreeSet<String>,
) -> Result<()> {
    let connection =
        Connection::open_with_flags(db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .with_context(|| format!("open {}", db_path.display()))?;
    collect_db_sessions(&connection, sessions)?;
    collect_db_messages(&connection, sessions, usage_events, seen_event_ids)
}

fn collect_db_sessions(
    connection: &Connection,
    sessions: &mut BTreeMap<String, SessionInfo>,
) -> Result<()> {
    let mut statement = connection
        .prepare("select id, directory, time_created from session order by time_created, id")?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?,
        ))
    })?;

    for row in rows {
        let (session_id, directory, time_created) = row?;
        upsert_session(
            sessions,
            &session_id,
            Some(date_from_millis(time_created)),
            Some(non_empty_or_unknown(&directory)),
            None,
        );
    }

    Ok(())
}

fn collect_db_messages(
    connection: &Connection,
    sessions: &mut BTreeMap<String, SessionInfo>,
    usage_events: &mut Vec<UsageEvent>,
    seen_event_ids: &mut BTreeSet<String>,
) -> Result<()> {
    let mut statement = connection.prepare(
        "select message.id,
                message.session_id,
                message.time_created,
                message.time_updated,
                message.data,
                session.directory,
                session.time_created
         from message
         left join session on session.id = message.session_id
         order by message.time_created, message.id",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(MessageRow {
            id: row.get(0)?,
            session_id: row.get(1)?,
            time_created: row.get(2)?,
            time_updated: row.get(3)?,
            data: row.get(4)?,
            session_directory: row.get(5)?,
            session_time_created: row.get(6)?,
        })
    })?;

    for row in rows {
        let row = row?;
        let value: Value = serde_json::from_str(&row.data)
            .with_context(|| format!("parse OpenCode message {}", row.id))?;
        collect_message_value(
            &value,
            MessageContext {
                event_id: row.id,
                fallback_session_id: row.session_id,
                fallback_project: row.session_directory.as_deref(),
                fallback_event_millis: Some(row.time_updated).or(Some(row.time_created)),
                fallback_session_millis: row.session_time_created.or(Some(row.time_created)),
            },
            sessions,
            usage_events,
            seen_event_ids,
        );
    }

    Ok(())
}

fn collect_session_files(
    session_root: &Path,
    sessions: &mut BTreeMap<String, SessionInfo>,
    warnings: &mut Vec<String>,
) {
    if !session_root.exists() {
        return;
    }

    for entry in WalkDir::new(session_root) {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                warnings.push(format!("Failed to walk OpenCode session data: {err}"));
                continue;
            }
        };
        if !entry.file_type().is_file() || !is_json(entry.path()) {
            continue;
        }

        match read_json(entry.path()) {
            Ok(value) => {
                let session_id =
                    string_at(&value, &["/id"]).unwrap_or_else(|| file_stem(entry.path()));
                let date =
                    millis_at(&value, &["/time/created", "/time/updated"]).map(date_from_millis);
                let project = string_at(&value, &["/directory"]);
                upsert_session(sessions, &session_id, date, project, None);
            }
            Err(err) => warnings.push(format!(
                "Failed to read OpenCode session file {}: {err:#}",
                entry.path().display()
            )),
        }
    }
}

fn collect_message_files(
    message_root: &Path,
    sessions: &mut BTreeMap<String, SessionInfo>,
    usage_events: &mut Vec<UsageEvent>,
    seen_event_ids: &mut BTreeSet<String>,
    warnings: &mut Vec<String>,
) {
    for entry in WalkDir::new(message_root) {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                warnings.push(format!("Failed to walk OpenCode data: {err}"));
                continue;
            }
        };
        if !entry.file_type().is_file() || !is_json(entry.path()) {
            continue;
        }

        match read_json(entry.path()) {
            Ok(value) => {
                collect_message_value(
                    &value,
                    MessageContext {
                        event_id: string_at(&value, &["/id"])
                            .unwrap_or_else(|| file_stem(entry.path())),
                        fallback_session_id: parent_dir_name(entry.path()),
                        fallback_project: None,
                        fallback_event_millis: None,
                        fallback_session_millis: None,
                    },
                    sessions,
                    usage_events,
                    seen_event_ids,
                );
            }
            Err(err) => warnings.push(format!(
                "Failed to read OpenCode message file {}: {err:#}",
                entry.path().display()
            )),
        }
    }
}

#[derive(Clone, Debug)]
struct MessageContext<'a> {
    event_id: String,
    fallback_session_id: String,
    fallback_project: Option<&'a str>,
    fallback_event_millis: Option<i64>,
    fallback_session_millis: Option<i64>,
}

fn collect_message_value(
    value: &Value,
    context: MessageContext<'_>,
    sessions: &mut BTreeMap<String, SessionInfo>,
    usage_events: &mut Vec<UsageEvent>,
    seen_event_ids: &mut BTreeSet<String>,
) {
    if value.get("role").and_then(Value::as_str) != Some("assistant") {
        return;
    }

    let session_id =
        string_at(value, &["/sessionID", "/session_id"]).unwrap_or(context.fallback_session_id);
    let event_millis =
        millis_at(value, &["/time/completed", "/time/created"]).or(context.fallback_event_millis);
    let session_millis = context.fallback_session_millis.or(event_millis);
    let date = event_millis
        .map(date_from_millis)
        .unwrap_or_else(|| String::from("unknown"));
    let session_date = session_millis.map(date_from_millis);
    let project = string_at(value, &["/path/cwd", "/path/root"])
        .or_else(|| context.fallback_project.map(non_empty_or_unknown))
        .unwrap_or_else(|| String::from("unknown"));
    let model = model_label(
        string_at(value, &["/providerID", "/model/providerID"]).as_deref(),
        string_at(value, &["/modelID", "/model/modelID"]).as_deref(),
    );

    upsert_session(
        sessions,
        &session_id,
        session_date,
        Some(project.clone()),
        Some(model.clone()),
    );

    let Some(tokens) = value.get("tokens") else {
        return;
    };
    if !seen_event_ids.insert(context.event_id.clone()) {
        return;
    }

    usage_events.push(UsageEvent {
        source: Source::OpenCode,
        event_id: context.event_id,
        session_id,
        date,
        project,
        model,
        usage: opencode_usage(tokens),
    });
}

fn upsert_session(
    sessions: &mut BTreeMap<String, SessionInfo>,
    session_id: &str,
    date: Option<String>,
    project: Option<String>,
    model: Option<String>,
) {
    let entry = sessions
        .entry(session_id.to_string())
        .or_insert_with(|| SessionInfo {
            date: date.clone().unwrap_or_else(|| String::from("unknown")),
            project: project.clone().unwrap_or_else(|| String::from("unknown")),
            model: model.clone().unwrap_or_else(|| String::from("unknown")),
        });

    if entry.date == "unknown" {
        if let Some(date) = date {
            entry.date = date;
        }
    }
    if entry.project == "unknown" {
        if let Some(project) = project {
            entry.project = project;
        }
    }
    if entry.model == "unknown" {
        if let Some(model) = model {
            entry.model = model;
        }
    }
}

fn opencode_usage(tokens: &Value) -> Usage {
    let input = get_u64(tokens, "input");
    let cached_input = tokens
        .get("cache")
        .map(|cache| get_u64(cache, "read"))
        .unwrap_or(0);
    let cache_creation_input = tokens
        .get("cache")
        .map(|cache| get_u64(cache, "write"))
        .unwrap_or(0);
    let output = get_u64(tokens, "output");
    let reasoning_output = get_u64(tokens, "reasoning");
    let total = (input + cached_input + cache_creation_input + output + reasoning_output)
        .max(get_u64(tokens, "total"));

    Usage {
        input,
        cached_input,
        cache_creation_input,
        cache_creation_input_5m: cache_creation_input,
        cache_creation_input_1h: 0,
        output,
        reasoning_output,
        total,
    }
}

fn model_label(provider: Option<&str>, model: Option<&str>) -> String {
    let model = model.map(str::trim).filter(|model| !model.is_empty());
    let Some(model) = model else {
        return String::from("unknown");
    };
    let provider = provider
        .map(str::trim)
        .filter(|provider| !provider.is_empty());
    match provider {
        Some(provider) => format!("{provider}/{model}"),
        None => model.to_string(),
    }
}

fn string_at(value: &Value, pointers: &[&str]) -> Option<String> {
    pointers
        .iter()
        .filter_map(|pointer| value.pointer(pointer).and_then(Value::as_str))
        .map(non_empty_or_unknown)
        .find(|value| value != "unknown")
}

fn millis_at(value: &Value, pointers: &[&str]) -> Option<i64> {
    pointers
        .iter()
        .find_map(|pointer| value.pointer(pointer).and_then(value_as_i64))
}

fn value_as_i64(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_u64().and_then(|value| i64::try_from(value).ok()))
}

fn date_from_millis(millis: i64) -> String {
    DateTime::from_timestamp_millis(millis)
        .map(|date| date.with_timezone(&Local).format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| String::from("unknown"))
}

fn read_json(path: &Path) -> Result<Value> {
    let content = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_str(&content).with_context(|| format!("parse {}", path.display()))
}

fn get_u64(value: &Value, key: &str) -> u64 {
    value.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn non_empty_or_unknown(value: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        String::from("unknown")
    } else {
        value.to_string()
    }
}

fn is_json(path: &Path) -> bool {
    path.extension().is_some_and(|ext| ext == "json")
}

fn file_stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("unknown")
        .to_string()
}

fn parent_dir_name(path: &Path) -> String {
    path.parent()
        .and_then(|parent| parent.file_name())
        .and_then(|name| name.to_str())
        .unwrap_or("unknown")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::{collect, date_from_millis, opencode_usage};
    use rusqlite::Connection;
    use serde_json::json;
    use std::fs;

    #[test]
    fn reads_opencode_token_fields() {
        let usage = opencode_usage(&json!({
            "total": 110_181,
            "input": 1,
            "output": 7_708,
            "reasoning": 5,
            "cache": {
                "read": 17_905,
                "write": 84_567
            }
        }));

        assert_eq!(usage.input, 1);
        assert_eq!(usage.cached_input, 17_905);
        assert_eq!(usage.cache_creation_input, 84_567);
        assert_eq!(usage.cache_creation_input_5m, 84_567);
        assert_eq!(usage.output, 7_708);
        assert_eq!(usage.reasoning_output, 5);
        assert_eq!(usage.computed_total(), 110_186);
    }

    #[test]
    fn reads_sqlite_and_message_storage_without_duplicate_events() {
        let root =
            std::env::temp_dir().join(format!("tokencheck-opencode-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("storage/message/ses_file")).unwrap();
        fs::create_dir_all(root.join("storage/session/global")).unwrap();

        let connection = Connection::open(root.join("opencode.db")).unwrap();
        connection
            .execute_batch(
                "create table session (
                    id text primary key,
                    directory text not null,
                    time_created integer not null
                );
                create table message (
                    id text primary key,
                    session_id text not null,
                    time_created integer not null,
                    time_updated integer not null,
                    data text not null
                );",
            )
            .unwrap();
        connection
            .execute(
                "insert into session (id, directory, time_created) values (?1, ?2, ?3)",
                ("ses_db", "/tmp/db-project", 1_771_094_670_993_i64),
            )
            .unwrap();
        let db_message = json!({
            "role": "assistant",
            "time": {"created": 1_771_094_671_004_i64, "completed": 1_771_094_673_209_i64},
            "model": {"providerID": "openai", "modelID": "gpt-5.3-codex"},
            "tokens": {
                "total": 23_370,
                "input": 195,
                "output": 519,
                "reasoning": 145,
                "cache": {"read": 22_656, "write": 0}
            }
        })
        .to_string();
        connection
            .execute(
                "insert into message (id, session_id, time_created, time_updated, data)
                 values (?1, ?2, ?3, ?4, ?5)",
                (
                    "msg_shared",
                    "ses_db",
                    1_771_094_671_004_i64,
                    1_771_094_673_209_i64,
                    db_message,
                ),
            )
            .unwrap();
        drop(connection);

        fs::write(
            root.join("storage/session/global/ses_file.json"),
            json!({
                "id": "ses_file",
                "directory": "/tmp/file-project",
                "time": {"created": 1_771_097_183_598_i64}
            })
            .to_string(),
        )
        .unwrap();
        fs::write(
            root.join("storage/message/ses_file/msg_file.json"),
            json!({
                "id": "msg_file",
                "sessionID": "ses_file",
                "role": "assistant",
                "time": {"created": 1_771_097_489_139_i64, "completed": 1_771_097_598_559_i64},
                "path": {"cwd": "/tmp/file-project"},
                "providerID": "anthropic",
                "modelID": "claude-opus-4-6",
                "tokens": {
                    "total": 110_181,
                    "input": 1,
                    "output": 7_708,
                    "reasoning": 0,
                    "cache": {"read": 17_905, "write": 84_567}
                }
            })
            .to_string(),
        )
        .unwrap();

        let data = collect(&root).unwrap();

        assert_eq!(data.sessions.len(), 2);
        assert_eq!(data.usage_events.len(), 2);
        assert_eq!(
            data.usage_events
                .iter()
                .map(|event| event.usage.computed_total())
                .sum::<u64>(),
            133_696
        );
        assert!(data
            .usage_events
            .iter()
            .any(|event| event.date == date_from_millis(1_771_097_598_559_i64)));
        assert!(data
            .usage_events
            .iter()
            .any(|event| event.model == "anthropic/claude-opus-4-6"));

        let _ = fs::remove_dir_all(root);
    }
}
