use crate::model::{ReportData, SessionMeta, Source, ToolEvent, UsageEvent};
use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MergeSummary {
    pub sessions_added: usize,
    pub usage_events_added: usize,
    pub usage_events_upgraded: usize,
    pub tool_events_added: usize,
}

impl MergeSummary {
    pub fn changed(self) -> bool {
        self.sessions_added > 0
            || self.usage_events_added > 0
            || self.usage_events_upgraded > 0
            || self.tool_events_added > 0
    }
}

pub fn load(path: &Path) -> Result<ReportData> {
    let content = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let mut data = serde_json::from_str::<ReportData>(&content)
        .with_context(|| format!("parse {}", path.display()))?;
    normalize_event_ids(&mut data);
    Ok(data)
}

pub fn load_or_default(path: &Path) -> Result<ReportData> {
    if path.exists() {
        load(path)
    } else {
        Ok(ReportData::default())
    }
}

pub fn save(path: &Path, data: &ReportData) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let mut content = serde_json::to_string_pretty(data)?;
    content.push('\n');
    let tmp_path = path.with_extension("tmp");
    fs::write(&tmp_path, content).with_context(|| format!("write {}", tmp_path.display()))?;
    fs::rename(&tmp_path, path)
        .with_context(|| format!("replace {} with {}", path.display(), tmp_path.display()))?;
    Ok(())
}

pub fn merge_preserving_growth(
    existing: &mut ReportData,
    mut incoming: ReportData,
) -> MergeSummary {
    normalize_event_ids(existing);
    normalize_event_ids(&mut incoming);

    let mut summary = MergeSummary::default();
    merge_sessions(existing, incoming.sessions, &mut summary);
    merge_usage_events(existing, incoming.usage_events, &mut summary);
    merge_tool_events(existing, incoming.tool_events, &mut summary);
    existing.warnings.clear();
    sort_data(existing);
    summary
}

fn normalize_event_ids(data: &mut ReportData) {
    for event in &mut data.usage_events {
        if event.event_id.is_empty() {
            event.event_id = usage_event_fallback_id(event);
        }
    }
    for event in &mut data.tool_events {
        if event.event_id.is_empty() {
            event.event_id = tool_event_fallback_id(event);
        }
    }
}

fn merge_sessions(
    existing: &mut ReportData,
    incoming: Vec<SessionMeta>,
    summary: &mut MergeSummary,
) {
    let mut sessions = existing
        .sessions
        .drain(..)
        .map(|session| (session_key(&session), session))
        .collect::<BTreeMap<_, _>>();
    for session in incoming {
        let key = session_key(&session);
        if let std::collections::btree_map::Entry::Vacant(entry) = sessions.entry(key) {
            entry.insert(session);
            summary.sessions_added += 1;
        }
    }
    existing.sessions = sessions.into_values().collect();
}

fn merge_usage_events(
    existing: &mut ReportData,
    incoming: Vec<UsageEvent>,
    summary: &mut MergeSummary,
) {
    let mut events = existing
        .usage_events
        .drain(..)
        .map(|event| (usage_event_key(&event), event))
        .collect::<BTreeMap<_, _>>();
    for event in incoming {
        let key = usage_event_key(&event);
        match events.get_mut(&key) {
            Some(existing) if event.usage.computed_total() > existing.usage.computed_total() => {
                *existing = event;
                summary.usage_events_upgraded += 1;
            }
            Some(_) => {}
            None => {
                events.insert(key, event);
                summary.usage_events_added += 1;
            }
        }
    }
    existing.usage_events = events.into_values().collect();
}

fn merge_tool_events(
    existing: &mut ReportData,
    incoming: Vec<ToolEvent>,
    summary: &mut MergeSummary,
) {
    let mut events = existing
        .tool_events
        .drain(..)
        .map(|event| (tool_event_key(&event), event))
        .collect::<BTreeMap<_, _>>();
    for event in incoming {
        let key = tool_event_key(&event);
        if let std::collections::btree_map::Entry::Vacant(entry) = events.entry(key) {
            entry.insert(event);
            summary.tool_events_added += 1;
        }
    }
    existing.tool_events = events.into_values().collect();
}

fn sort_data(data: &mut ReportData) {
    data.sessions.sort_by_key(session_key);
    data.usage_events.sort_by_key(usage_event_key);
    data.tool_events.sort_by_key(tool_event_key);
}

fn session_key(session: &SessionMeta) -> (Source, String) {
    (session.source, session.session_id.clone())
}

fn usage_event_key(event: &UsageEvent) -> (Source, String) {
    (event.source, event.event_id.clone())
}

fn tool_event_key(event: &ToolEvent) -> (Source, String) {
    (event.source, event.event_id.clone())
}

fn usage_event_fallback_id(event: &UsageEvent) -> String {
    format!(
        "{}:{}:{}:{}:{}:{}",
        event.source.label(),
        event.session_id,
        event.date,
        event.project,
        event.model,
        event.usage.computed_total()
    )
}

fn tool_event_fallback_id(event: &ToolEvent) -> String {
    format!(
        "{}:{}:{}:{}",
        event.source.label(),
        event.date,
        event.project,
        event.tool
    )
}

#[cfg(test)]
mod tests {
    use super::merge_preserving_growth;
    use crate::model::{ReportData, Source, Usage, UsageEvent};

    #[test]
    fn keeps_existing_usage_when_incoming_is_smaller() {
        let mut existing = ReportData {
            usage_events: vec![usage_event("event-1", 100)],
            ..ReportData::default()
        };
        let incoming = ReportData {
            usage_events: vec![usage_event("event-1", 50)],
            ..ReportData::default()
        };

        let summary = merge_preserving_growth(&mut existing, incoming);

        assert!(!summary.changed());
        assert_eq!(existing.usage_events[0].usage.computed_total(), 100);
    }

    #[test]
    fn upgrades_existing_usage_when_incoming_is_larger() {
        let mut existing = ReportData {
            usage_events: vec![usage_event("event-1", 100)],
            ..ReportData::default()
        };
        let incoming = ReportData {
            usage_events: vec![usage_event("event-1", 150)],
            ..ReportData::default()
        };

        let summary = merge_preserving_growth(&mut existing, incoming);

        assert_eq!(summary.usage_events_upgraded, 1);
        assert_eq!(existing.usage_events[0].usage.computed_total(), 150);
    }

    #[test]
    fn adds_new_usage_events_without_dropping_existing_events() {
        let mut existing = ReportData {
            usage_events: vec![usage_event("event-1", 100)],
            ..ReportData::default()
        };
        let incoming = ReportData {
            usage_events: vec![usage_event("event-2", 200)],
            ..ReportData::default()
        };

        let summary = merge_preserving_growth(&mut existing, incoming);

        assert_eq!(summary.usage_events_added, 1);
        assert_eq!(existing.usage_events.len(), 2);
    }

    fn usage_event(event_id: &str, total: u64) -> UsageEvent {
        UsageEvent {
            source: Source::Codex,
            event_id: event_id.to_string(),
            session_id: String::from("session"),
            date: String::from("2026-03-01"),
            project: String::from("/tmp/project"),
            model: String::from("gpt-5.4"),
            usage: Usage {
                total,
                ..Usage::default()
            },
        }
    }
}
