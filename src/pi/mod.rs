use crate::model::{ReportData, SessionMeta, Source, ToolEvent, Usage, UsageEvent};
use anyhow::{Context, Result};
use serde::Deserialize;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use walkdir::WalkDir;

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum PiEntry {
    Session {
        id: String,
        timestamp: String,
        cwd: String,
    },
    Message {
        id: String,
        timestamp: String,
        message: PiMessage,
    },
    Compaction {
        id: String,
        timestamp: String,
        #[serde(default)]
        usage: Option<PiUsage>,
    },
    BranchSummary {
        id: String,
        timestamp: String,
        #[serde(default)]
        usage: Option<PiUsage>,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "role")]
enum PiMessage {
    #[serde(rename = "assistant")]
    Assistant {
        #[serde(default)]
        content: Vec<PiContent>,
        provider: String,
        model: String,
        #[serde(default, rename = "responseModel")]
        response_model: Option<String>,
        usage: PiUsage,
    },
    #[serde(rename = "toolResult")]
    ToolResult {
        #[serde(default)]
        usage: Option<PiUsage>,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum PiContent {
    #[serde(rename = "toolCall")]
    ToolCall {
        #[serde(default)]
        id: String,
        name: String,
    },
    #[serde(other)]
    Other,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PiUsage {
    #[serde(default)]
    input: u64,
    #[serde(default)]
    output: u64,
    #[serde(default)]
    cache_read: u64,
    #[serde(default)]
    cache_write: u64,
    #[serde(default)]
    cache_write_1h: u64,
    #[serde(default)]
    total_tokens: u64,
}

impl PiUsage {
    fn into_usage(self) -> Usage {
        let cache_creation_input_1h = self.cache_write_1h.min(self.cache_write);
        let cache_creation_input_5m = self.cache_write.saturating_sub(cache_creation_input_1h);
        let computed_total = self
            .input
            .saturating_add(self.output)
            .saturating_add(self.cache_read)
            .saturating_add(self.cache_write);

        Usage {
            input: self.input,
            cached_input: self.cache_read,
            cache_creation_input: self.cache_write,
            cache_creation_input_5m,
            cache_creation_input_1h,
            output: self.output,
            reasoning_output: 0,
            total: self.total_tokens.max(computed_total),
        }
    }
}

pub fn collect(sessions_root: &Path) -> Result<ReportData> {
    let mut data = ReportData::default();
    if !sessions_root.exists() {
        data.warnings.push(format!(
            "Pi data directory not found: {}",
            sessions_root.display()
        ));
        return Ok(data);
    }

    for entry in WalkDir::new(sessions_root) {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                data.warnings.push(format!("Failed to walk Pi data: {err}"));
                continue;
            }
        };
        if !entry.file_type().is_file() || !is_jsonl(entry.path()) {
            continue;
        }

        match collect_file(entry.path()) {
            Ok(file_data) => data.merge(file_data),
            Err(err) => data.warnings.push(format!(
                "Failed to read Pi file {}: {err:#}",
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
    let mut session_project = String::from("unknown");
    let mut session_model = String::from("unknown");
    let mut usage_events = Vec::new();
    let mut tool_events = Vec::new();
    let mut warnings = Vec::new();

    for (line_index, line) in reader.lines().enumerate() {
        let line =
            line.with_context(|| format!("read line {} in {}", line_index + 1, path.display()))?;
        if line.trim().is_empty() {
            continue;
        }

        let entry = match serde_json::from_str::<PiEntry>(&line) {
            Ok(entry) => entry,
            Err(err) => {
                warnings.push(format!(
                    "Invalid JSON in {} line {}: {err}",
                    path.display(),
                    line_index + 1
                ));
                continue;
            }
        };

        match entry {
            PiEntry::Session { id, timestamp, cwd } => {
                session_id = id;
                session_date = date_from_timestamp(&timestamp);
                session_project = non_empty_or_unknown(&cwd);
            }
            PiEntry::Message {
                id,
                timestamp,
                message,
            } => match message {
                PiMessage::Assistant {
                    content,
                    provider,
                    model,
                    response_model,
                    usage,
                } => {
                    let model = model_label(&provider, response_model.as_deref().unwrap_or(&model));
                    session_model = model.clone();
                    let date = date_from_timestamp(&timestamp);

                    for (item_index, item) in content.into_iter().enumerate() {
                        let PiContent::ToolCall { id: tool_id, name } = item else {
                            continue;
                        };
                        let tool_id = if tool_id.is_empty() {
                            format!("{id}:{item_index}")
                        } else {
                            tool_id
                        };
                        tool_events.push(ToolEvent {
                            source: Source::Pi,
                            event_id: format!("{session_id}:{tool_id}"),
                            date: date.clone(),
                            project: session_project.clone(),
                            tool: name,
                        });
                    }

                    push_usage_event(
                        &mut usage_events,
                        &session_id,
                        &session_project,
                        &id,
                        &timestamp,
                        model,
                        usage,
                    );
                }
                PiMessage::ToolResult { usage } => {
                    if let Some(usage) = usage {
                        push_usage_event(
                            &mut usage_events,
                            &session_id,
                            &session_project,
                            &id,
                            &timestamp,
                            String::from("tools/summaries"),
                            usage,
                        );
                    }
                }
                PiMessage::Other => {}
            },
            PiEntry::Compaction {
                id,
                timestamp,
                usage,
            }
            | PiEntry::BranchSummary {
                id,
                timestamp,
                usage,
            } => {
                if let Some(usage) = usage {
                    push_usage_event(
                        &mut usage_events,
                        &session_id,
                        &session_project,
                        &id,
                        &timestamp,
                        String::from("tools/summaries"),
                        usage,
                    );
                }
            }
            PiEntry::Other => {}
        }
    }

    let mut data = ReportData::default();
    data.sessions.push(SessionMeta {
        source: Source::Pi,
        session_id,
        date: session_date,
        project: session_project,
        model: session_model,
    });
    data.usage_events = usage_events;
    data.tool_events = tool_events;
    data.warnings = warnings;
    Ok(data)
}

fn push_usage_event(
    events: &mut Vec<UsageEvent>,
    session_id: &str,
    project: &str,
    entry_id: &str,
    timestamp: &str,
    model: String,
    usage: PiUsage,
) {
    let usage = usage.into_usage();
    if usage.computed_total() == 0 {
        return;
    }
    events.push(UsageEvent {
        source: Source::Pi,
        event_id: format!("{session_id}:{entry_id}"),
        session_id: session_id.to_string(),
        date: date_from_timestamp(timestamp),
        project: project.to_string(),
        model,
        usage,
    });
}

fn model_label(provider: &str, model: &str) -> String {
    let provider = provider.trim();
    let model = model.trim();
    if model.is_empty() {
        String::from("unknown")
    } else if provider.is_empty() {
        model.to_string()
    } else {
        format!("{provider}/{model}")
    }
}

fn non_empty_or_unknown(value: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        String::from("unknown")
    } else {
        value.to_string()
    }
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

fn is_jsonl(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension == "jsonl")
}

#[cfg(test)]
mod tests {
    use super::collect_file;
    use crate::model::Source;
    use serde_json::json;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEMP_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn collects_pi_usage_and_tool_metadata_without_message_content() {
        // Given
        let counter = TEMP_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("tokencheck-pi-{}-{counter}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("session.jsonl");
        let content = [
            json!({
                "type": "session",
                "version": 3,
                "id": "session-1",
                "timestamp": "2026-07-29T08:00:00.000Z",
                "cwd": "/tmp/pi-project"
            }),
            json!({
                "type": "message",
                "id": "assistant-1",
                "parentId": null,
                "timestamp": "2026-07-29T08:01:00.000Z",
                "message": {
                    "role": "assistant",
                    "content": [
                        {"type": "text", "text": "private response"},
                        {
                            "type": "toolCall",
                            "id": "call-1",
                            "name": "bash",
                            "arguments": {"command": "private command"}
                        }
                    ],
                    "provider": "provider",
                    "model": "requested-model",
                    "responseModel": "actual-model",
                    "usage": {
                        "input": 10,
                        "output": 20,
                        "cacheRead": 30,
                        "cacheWrite": 40,
                        "cacheWrite1h": 8,
                        "reasoning": 5,
                        "totalTokens": 100
                    }
                }
            }),
            json!({
                "type": "message",
                "id": "tool-result-1",
                "parentId": "assistant-1",
                "timestamp": "2026-07-29T08:02:00.000Z",
                "message": {
                    "role": "toolResult",
                    "toolCallId": "call-1",
                    "toolName": "bash",
                    "content": [{"type": "text", "text": "private tool output"}],
                    "usage": {
                        "input": 1,
                        "output": 2,
                        "cacheRead": 3,
                        "cacheWrite": 4,
                        "totalTokens": 10
                    }
                }
            }),
            json!({
                "type": "compaction",
                "id": "compaction-1",
                "parentId": "tool-result-1",
                "timestamp": "2026-07-29T08:03:00.000Z",
                "summary": "private summary",
                "usage": {
                    "input": 5,
                    "output": 6,
                    "cacheRead": 7,
                    "cacheWrite": 8,
                    "totalTokens": 26
                }
            }),
            json!({
                "type": "branch_summary",
                "id": "branch-1",
                "parentId": "compaction-1",
                "timestamp": "2026-07-29T08:04:00.000Z",
                "fromId": "assistant-1",
                "summary": "private branch summary",
                "usage": {
                    "input": 9,
                    "output": 10,
                    "cacheRead": 11,
                    "cacheWrite": 12,
                    "totalTokens": 42
                }
            }),
        ]
        .into_iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>()
        .join("\n");
        fs::write(&path, format!("{content}\n")).unwrap();

        // When
        let data = collect_file(&path).unwrap();

        // Then
        assert_eq!(data.sessions.len(), 1);
        assert_eq!(data.sessions[0].source, Source::Pi);
        assert_eq!(data.sessions[0].session_id, "session-1");
        assert_eq!(data.sessions[0].date, "2026-07-29");
        assert_eq!(data.sessions[0].project, "/tmp/pi-project");
        assert_eq!(data.sessions[0].model, "provider/actual-model");
        assert_eq!(data.usage_events.len(), 4);
        assert_eq!(data.usage_events[0].model, "provider/actual-model");
        assert_eq!(data.usage_events[0].usage.input, 10);
        assert_eq!(data.usage_events[0].usage.output, 20);
        assert_eq!(data.usage_events[0].usage.cached_input, 30);
        assert_eq!(data.usage_events[0].usage.cache_creation_input, 40);
        assert_eq!(data.usage_events[0].usage.cache_creation_input_5m, 32);
        assert_eq!(data.usage_events[0].usage.cache_creation_input_1h, 8);
        assert_eq!(data.usage_events[0].usage.reasoning_output, 0);
        assert_eq!(data.usage_events[0].usage.computed_total(), 100);
        assert_eq!(data.usage_events[1].model, "tools/summaries");
        assert_eq!(data.usage_events[2].model, "tools/summaries");
        assert_eq!(data.usage_events[3].model, "tools/summaries");
        assert_eq!(data.tool_events.len(), 1);
        assert_eq!(data.tool_events[0].source, Source::Pi);
        assert_eq!(data.tool_events[0].tool, "bash");

        fs::remove_dir_all(root).unwrap();
    }
}
