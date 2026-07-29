use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Ord, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Claude,
    Codex,
    OpenCode,
    Pi,
}

impl Source {
    pub fn label(self) -> &'static str {
        match self {
            Source::Claude => "claude",
            Source::Codex => "codex",
            Source::OpenCode => "opencode",
            Source::Pi => "pi",
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Usage {
    pub input: u64,
    pub cached_input: u64,
    pub cache_creation_input: u64,
    #[serde(default)]
    pub cache_creation_input_5m: u64,
    #[serde(default)]
    pub cache_creation_input_1h: u64,
    pub output: u64,
    pub reasoning_output: u64,
    pub total: u64,
}

impl Usage {
    pub fn add_assign(&mut self, other: &Usage) {
        self.input += other.input;
        self.cached_input += other.cached_input;
        self.cache_creation_input += other.cache_creation_input;
        self.cache_creation_input_5m += other.cache_creation_input_5m;
        self.cache_creation_input_1h += other.cache_creation_input_1h;
        self.output += other.output;
        self.reasoning_output += other.reasoning_output;
        self.total += other.total;
    }

    pub fn computed_total(&self) -> u64 {
        if self.total > 0 {
            self.total
        } else {
            self.input
                + self.cached_input
                + self.cache_creation_input
                + self.output
                + self.reasoning_output
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SessionMeta {
    pub source: Source,
    pub session_id: String,
    pub date: String,
    pub project: String,
    pub model: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UsageEvent {
    pub source: Source,
    #[serde(default)]
    pub event_id: String,
    pub session_id: String,
    pub date: String,
    pub project: String,
    pub model: String,
    pub usage: Usage,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ToolEvent {
    pub source: Source,
    #[serde(default)]
    pub event_id: String,
    pub date: String,
    pub project: String,
    pub tool: String,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct ReportData {
    #[serde(default)]
    pub sessions: Vec<SessionMeta>,
    #[serde(default)]
    pub usage_events: Vec<UsageEvent>,
    #[serde(default)]
    pub tool_events: Vec<ToolEvent>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

impl ReportData {
    pub fn merge(&mut self, mut other: ReportData) {
        self.sessions.append(&mut other.sessions);
        self.usage_events.append(&mut other.usage_events);
        self.tool_events.append(&mut other.tool_events);
        self.warnings.append(&mut other.warnings);
    }
}

#[derive(Clone, Debug)]
pub struct Roots {
    pub home: PathBuf,
}

impl Roots {
    pub fn claude_projects(&self) -> PathBuf {
        self.home.join(".claude").join("projects")
    }

    pub fn codex_sessions(&self) -> PathBuf {
        self.home.join(".codex").join("sessions")
    }

    pub fn opencode_root(&self) -> PathBuf {
        self.home.join(".local").join("share").join("opencode")
    }

    pub fn pi_sessions(&self) -> PathBuf {
        self.home.join(".pi").join("agent").join("sessions")
    }
}

pub type UsageMap<K> = BTreeMap<K, Usage>;

#[cfg(test)]
mod tests {
    use super::{Roots, Source, Usage};
    use std::path::PathBuf;

    #[test]
    fn labels_pi_source_for_reports_and_snapshots() {
        // Given
        let source = Source::Pi;

        // When
        let label = source.label();
        let serialized = serde_json::to_string(&source).unwrap();

        // Then
        assert_eq!(label, "pi");
        assert_eq!(serialized, r#""pi""#);
    }

    #[test]
    fn resolves_pi_sessions_under_agent_data_directory() {
        // Given
        let roots = Roots {
            home: PathBuf::from("/tmp/user"),
        };

        // When
        let sessions = roots.pi_sessions();

        // Then
        assert_eq!(sessions, PathBuf::from("/tmp/user/.pi/agent/sessions"));
    }

    #[test]
    fn uses_explicit_total_when_present() {
        let usage = Usage {
            input: 10,
            cached_input: 20,
            cache_creation_input: 30,
            cache_creation_input_5m: 0,
            cache_creation_input_1h: 0,
            output: 40,
            reasoning_output: 50,
            total: 70,
        };

        assert_eq!(usage.computed_total(), 70);
    }

    #[test]
    fn computes_total_from_canonical_fields_when_missing() {
        let usage = Usage {
            input: 10,
            cached_input: 20,
            cache_creation_input: 30,
            cache_creation_input_5m: 0,
            cache_creation_input_1h: 0,
            output: 40,
            reasoning_output: 50,
            total: 0,
        };

        assert_eq!(usage.computed_total(), 150);
    }
}
