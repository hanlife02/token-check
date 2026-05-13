use crate::model::{ReportData, SessionMeta, Source, ToolEvent, Usage, UsageEvent};
use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use walkdir::WalkDir;

struct SeenUsage {
    timestamp: String,
    event: UsageEvent,
}

pub fn collect(projects_root: &Path) -> Result<ReportData> {
    let mut data = ReportData::default();
    if !projects_root.exists() {
        data.warnings.push(format!(
            "Claude Code data directory not found: {}",
            projects_root.display()
        ));
        return Ok(data);
    }

    for entry in WalkDir::new(projects_root) {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                data.warnings
                    .push(format!("Failed to walk Claude Code data: {err}"));
                continue;
            }
        };
        if !entry.file_type().is_file() || !is_jsonl(entry.path()) {
            continue;
        }

        match collect_file(entry.path()) {
            Ok(file_data) => data.merge(file_data),
            Err(err) => data.warnings.push(format!(
                "Failed to read Claude Code file {}: {err:#}",
                entry.path().display()
            )),
        }
    }

    Ok(data)
}

fn collect_file(path: &Path) -> Result<ReportData> {
    let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let reader = BufReader::new(file);
    let mut session_id = file_stem(path);
    let mut session_date = String::from("unknown");
    let mut session_project = project_from_dir(path);
    let mut session_model = String::from("unknown");
    let mut usage_by_message: HashMap<String, SeenUsage> = HashMap::new();
    let mut tool_ids = HashSet::new();
    let mut tool_events = Vec::new();
    let mut warnings = Vec::new();

    for (line_index, line) in reader.lines().enumerate() {
        let line =
            line.with_context(|| format!("read line {} in {}", line_index + 1, path.display()))?;
        if line.trim().is_empty() {
            continue;
        }

        let value: Value = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(err) => {
                warnings.push(format!(
                    "Invalid JSON in {} line {}: {err}",
                    path.display(),
                    line_index + 1
                ));
                continue;
            }
        };

        if let Some(id) = value.get("sessionId").and_then(Value::as_str) {
            session_id = id.to_string();
        }
        if let Some(timestamp) = value.get("timestamp").and_then(Value::as_str) {
            if session_date == "unknown" {
                session_date = date_from_timestamp(timestamp);
            }
        }
        if let Some(cwd) = value.get("cwd").and_then(Value::as_str) {
            if !cwd.is_empty() {
                session_project = cwd.to_string();
            }
        }

        collect_tool_events(
            &value,
            &mut tool_ids,
            &mut tool_events,
            &session_id,
            &session_project,
            line_index,
        );

        let Some(usage_value) = value.pointer("/message/usage") else {
            continue;
        };

        let message_id = value
            .pointer("/message/id")
            .and_then(Value::as_str)
            .or_else(|| value.get("uuid").and_then(Value::as_str))
            .map(str::to_string)
            .unwrap_or_else(|| format!("{}:{}", path.display(), line_index));
        let key = format!("{session_id}:{message_id}");
        let timestamp = value
            .get("timestamp")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let date = if timestamp.is_empty() {
            session_date.clone()
        } else {
            date_from_timestamp(&timestamp)
        };
        let project = value
            .get("cwd")
            .and_then(Value::as_str)
            .filter(|cwd| !cwd.is_empty())
            .unwrap_or(&session_project)
            .to_string();
        let model = value
            .pointer("/message/model")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        if model != "unknown" {
            session_model = model.clone();
        }

        let usage = claude_usage(usage_value);
        let event = UsageEvent {
            source: Source::Claude,
            event_id: key.clone(),
            session_id: session_id.clone(),
            date,
            project,
            model,
            usage,
        };

        match usage_by_message.get(&key) {
            Some(existing) if existing.timestamp > timestamp => {}
            _ => {
                usage_by_message.insert(key, SeenUsage { timestamp, event });
            }
        }
    }

    let mut data = ReportData::default();
    data.sessions.push(SessionMeta {
        source: Source::Claude,
        session_id,
        date: session_date,
        project: session_project,
        model: session_model,
    });
    data.usage_events
        .extend(usage_by_message.into_values().map(|seen| seen.event));
    data.tool_events = tool_events;
    data.warnings = warnings;
    Ok(data)
}

fn claude_usage(value: &Value) -> Usage {
    let input = get_u64(value, "input_tokens");
    let cached_input = get_u64(value, "cache_read_input_tokens");
    let cache_creation_input_5m = get_u64(value, "cache_creation_input_tokens")
        + get_u64(value, "claude_cache_creation_5_m_tokens");
    let cache_creation_input_1h = get_u64(value, "claude_cache_creation_1_h_tokens");
    let cache_creation_input = cache_creation_input_5m + cache_creation_input_1h;
    let output = get_u64(value, "output_tokens");

    Usage {
        input,
        cached_input,
        cache_creation_input,
        cache_creation_input_5m,
        cache_creation_input_1h,
        output,
        reasoning_output: 0,
        total: input + cached_input + cache_creation_input + output,
    }
}

fn collect_tool_events(
    value: &Value,
    tool_ids: &mut HashSet<String>,
    tool_events: &mut Vec<ToolEvent>,
    session_id: &str,
    project: &str,
    line_index: usize,
) {
    let Some(content) = value.pointer("/message/content").and_then(Value::as_array) else {
        return;
    };
    let date = value
        .get("timestamp")
        .and_then(Value::as_str)
        .map(date_from_timestamp)
        .unwrap_or_else(|| String::from("unknown"));

    for (item_index, item) in content.iter().enumerate() {
        if item.get("type").and_then(Value::as_str) != Some("tool_use") {
            continue;
        }
        let Some(name) = item.get("name").and_then(Value::as_str) else {
            continue;
        };
        let id = item
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| format!("line:{line_index}:item:{item_index}"));
        let event_id = format!("{session_id}:{id}");
        if tool_ids.insert(event_id.clone()) {
            tool_events.push(ToolEvent {
                source: Source::Claude,
                event_id,
                date: date.clone(),
                project: project.to_string(),
                tool: name.to_string(),
            });
        }
    }
}

fn get_u64(value: &Value, key: &str) -> u64 {
    value.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn is_jsonl(path: &Path) -> bool {
    path.extension().is_some_and(|ext| ext == "jsonl")
}

fn date_from_timestamp(timestamp: &str) -> String {
    timestamp.get(0..10).unwrap_or("unknown").to_string()
}

fn file_stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("unknown")
        .to_string()
}

fn project_from_dir(path: &Path) -> String {
    path.parent()
        .and_then(|parent| parent.file_name())
        .and_then(|name| name.to_str())
        .map(|name| name.trim_start_matches('-').replace('-', "/"))
        .unwrap_or_else(|| String::from("unknown"))
}

#[cfg(test)]
mod tests {
    use super::{claude_usage, is_jsonl};
    use serde_json::json;
    use std::path::Path;

    #[test]
    fn reads_claude_usage_fields_without_message_content() {
        let usage = claude_usage(&json!({
            "input_tokens": 10,
            "cache_read_input_tokens": 20,
            "cache_creation_input_tokens": 30,
            "claude_cache_creation_1_h_tokens": 40,
            "claude_cache_creation_5_m_tokens": 50,
            "output_tokens": 60
        }));

        assert_eq!(usage.input, 10);
        assert_eq!(usage.cached_input, 20);
        assert_eq!(usage.cache_creation_input, 120);
        assert_eq!(usage.cache_creation_input_5m, 80);
        assert_eq!(usage.cache_creation_input_1h, 40);
        assert_eq!(usage.output, 60);
        assert_eq!(usage.computed_total(), 210);
    }

    #[test]
    fn recognizes_jsonl_files_only() {
        assert!(is_jsonl(Path::new("session.jsonl")));
        assert!(!is_jsonl(Path::new("session.json")));
        assert!(!is_jsonl(Path::new("session")));
    }
}
