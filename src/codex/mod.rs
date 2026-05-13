use crate::model::{ReportData, SessionMeta, Source, ToolEvent, Usage, UsageEvent};
use anyhow::{Context, Result};
use serde_json::Value;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use walkdir::WalkDir;

pub fn collect(sessions_root: &Path) -> Result<ReportData> {
    let mut data = ReportData::default();
    if !sessions_root.exists() {
        data.warnings.push(format!(
            "Codex data directory not found: {}",
            sessions_root.display()
        ));
        return Ok(data);
    }

    for entry in WalkDir::new(sessions_root) {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                data.warnings
                    .push(format!("Failed to walk Codex data: {err}"));
                continue;
            }
        };
        if !entry.file_type().is_file() || !is_jsonl(entry.path()) {
            continue;
        }

        match collect_file(entry.path()) {
            Ok(file_data) => data.merge(file_data),
            Err(err) => data.warnings.push(format!(
                "Failed to read Codex file {}: {err:#}",
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
    let mut date = date_from_path(path).unwrap_or_else(|| String::from("unknown"));
    let mut project = String::from("unknown");
    let mut model = String::from("unknown");
    let mut provider = String::new();
    let mut final_usage: Option<(String, Usage)> = None;
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

        if let Some(timestamp) = value.get("timestamp").and_then(Value::as_str) {
            if date == "unknown" {
                date = date_from_timestamp(timestamp);
            }
        }

        match value.get("type").and_then(Value::as_str) {
            Some("session_meta") => {
                if let Some(payload) = value.get("payload") {
                    if let Some(id) = payload.get("id").and_then(Value::as_str) {
                        session_id = id.to_string();
                    }
                    if let Some(cwd) = payload.get("cwd").and_then(Value::as_str) {
                        project = cwd.to_string();
                    }
                    if let Some(model_provider) =
                        payload.get("model_provider").and_then(Value::as_str)
                    {
                        provider = model_provider.to_string();
                    }
                }
            }
            Some("turn_context") => {
                if let Some(payload) = value.get("payload") {
                    if let Some(cwd) = payload.get("cwd").and_then(Value::as_str) {
                        project = cwd.to_string();
                    }
                    if let Some(turn_model) = payload.get("model").and_then(Value::as_str) {
                        model = turn_model.to_string();
                    }
                }
            }
            Some("event_msg") => {
                if let Some(payload) = value.get("payload") {
                    if payload.get("type").and_then(Value::as_str) == Some("token_count") {
                        if let Some(usage_value) = payload.pointer("/info/total_token_usage") {
                            let usage_date = value
                                .get("timestamp")
                                .and_then(Value::as_str)
                                .map(date_from_timestamp)
                                .unwrap_or_else(|| date.clone());
                            final_usage = Some((usage_date, codex_usage(usage_value)));
                        }
                    }
                }
            }
            Some("response_item") => {
                if let Some(tool) = tool_name(value.get("payload")) {
                    let event_date = value
                        .get("timestamp")
                        .and_then(Value::as_str)
                        .map(date_from_timestamp)
                        .unwrap_or_else(|| date.clone());
                    tool_events.push(ToolEvent {
                        source: Source::Codex,
                        date: event_date,
                        project: project.clone(),
                        tool,
                    });
                }
            }
            _ => {}
        }
    }

    let model_label = if provider.is_empty() || model == "unknown" {
        model.clone()
    } else {
        format!("{provider}/{model}")
    };

    let mut data = ReportData::default();
    data.sessions.push(SessionMeta {
        source: Source::Codex,
        session_id: session_id.clone(),
        date: date.clone(),
        project: project.clone(),
        model: model_label.clone(),
    });
    if let Some((usage_date, usage)) = final_usage {
        data.usage_events.push(UsageEvent {
            source: Source::Codex,
            session_id,
            date: usage_date,
            project,
            model: model_label,
            usage,
        });
    }
    data.tool_events = tool_events;
    data.warnings = warnings;
    Ok(data)
}

fn codex_usage(value: &Value) -> Usage {
    let input = get_u64(value, "input_tokens");
    let cached_input = get_u64(value, "cached_input_tokens");
    let output = get_u64(value, "output_tokens");
    let reasoning_output = get_u64(value, "reasoning_output_tokens");
    let total = get_u64(value, "total_tokens");

    Usage {
        input,
        cached_input,
        cache_creation_input: 0,
        output,
        reasoning_output,
        total: if total > 0 { total } else { input + output },
    }
}

fn tool_name(payload: Option<&Value>) -> Option<String> {
    let payload = payload?;
    match payload.get("type").and_then(Value::as_str)? {
        "function_call" => payload
            .get("name")
            .and_then(Value::as_str)
            .map(str::to_string),
        "custom_tool_call" => payload
            .get("name")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| Some(String::from("custom_tool_call"))),
        "web_search_call" => Some(String::from("web_search")),
        _ => None,
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

fn date_from_path(path: &Path) -> Option<String> {
    let parts: Vec<_> = path
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .collect();
    for window in parts.windows(3) {
        let [year, month, day] = window else {
            continue;
        };
        if year.len() == 4
            && month.len() == 2
            && day.len() == 2
            && year.chars().all(|ch| ch.is_ascii_digit())
            && month.chars().all(|ch| ch.is_ascii_digit())
            && day.chars().all(|ch| ch.is_ascii_digit())
        {
            return Some(format!("{year}-{month}-{day}"));
        }
    }
    None
}

fn file_stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("unknown")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::{codex_usage, date_from_path, is_jsonl, tool_name};
    use serde_json::json;
    use std::path::Path;

    #[test]
    fn reads_codex_usage_fields_without_message_content() {
        let usage = codex_usage(&json!({
            "input_tokens": 10,
            "cached_input_tokens": 20,
            "output_tokens": 30,
            "reasoning_output_tokens": 40,
            "total_tokens": 50
        }));

        assert_eq!(usage.input, 10);
        assert_eq!(usage.cached_input, 20);
        assert_eq!(usage.output, 30);
        assert_eq!(usage.reasoning_output, 40);
        assert_eq!(usage.computed_total(), 50);
    }

    #[test]
    fn extracts_session_date_from_codex_path() {
        let path = Path::new("/home/user/.codex/sessions/2026/05/13/session.jsonl");

        assert_eq!(date_from_path(path).as_deref(), Some("2026-05-13"));
    }

    #[test]
    fn maps_tool_event_names_from_response_items() {
        assert_eq!(
            tool_name(Some(
                &json!({"type": "function_call", "name": "shell_command"})
            )),
            Some(String::from("shell_command"))
        );
        assert_eq!(
            tool_name(Some(&json!({"type": "custom_tool_call"}))),
            Some(String::from("custom_tool_call"))
        );
        assert_eq!(
            tool_name(Some(&json!({"type": "web_search_call"}))),
            Some(String::from("web_search"))
        );
        assert_eq!(tool_name(Some(&json!({"type": "message"}))), None);
    }

    #[test]
    fn recognizes_jsonl_files_only() {
        assert!(is_jsonl(Path::new("session.jsonl")));
        assert!(!is_jsonl(Path::new("session.json")));
        assert!(!is_jsonl(Path::new("session")));
    }
}
