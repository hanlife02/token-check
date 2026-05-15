use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

pub const DEFAULT_DATA_FILE: &str = "data/tokencheck.json";
pub const DEFAULT_LIMIT: usize = 20;
pub const DEFAULT_HEATMAP_MONTHS: usize = 12;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct AppConfig {
    pub language: Language,
    pub source: SourcePreference,
    pub data_file: PathBuf,
    pub limit: usize,
    pub heatmap_months: usize,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            language: Language::default(),
            source: SourcePreference::default(),
            data_file: PathBuf::from(DEFAULT_DATA_FILE),
            limit: DEFAULT_LIMIT,
            heatmap_months: DEFAULT_HEATMAP_MONTHS,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    #[default]
    En,
    Zh,
}

impl Language {
    pub fn as_str(self) -> &'static str {
        match self {
            Language::En => "en",
            Language::Zh => "zh",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Language::En => "English",
            Language::Zh => "中文",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "en" | "eng" | "english" | "e" | "英文" => Some(Language::En),
            "zh" | "cn" | "zho" | "chinese" | "中文" | "汉语" | "漢語" => Some(Language::Zh),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SourcePreference {
    #[default]
    All,
    Claude,
    Codex,
}

impl SourcePreference {
    pub fn as_str(self) -> &'static str {
        match self {
            SourcePreference::All => "all",
            SourcePreference::Claude => "claude",
            SourcePreference::Codex => "codex",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "all" | "a" | "全部" | "所有" => Some(SourcePreference::All),
            "claude" | "c" => Some(SourcePreference::Claude),
            "codex" | "x" => Some(SourcePreference::Codex),
            _ => None,
        }
    }
}

pub fn load_or_default() -> Result<AppConfig> {
    let path = config_path()?;
    load_or_default_from(&path)
}

pub fn save(config: &AppConfig) -> Result<PathBuf> {
    let path = config_path()?;
    save_to_path(&path, config)?;
    Ok(path)
}

pub fn reset() -> Result<(PathBuf, bool)> {
    let path = config_path()?;
    if path.exists() {
        fs::remove_file(&path).with_context(|| format!("remove {}", path.display()))?;
        return Ok((path, true));
    }
    Ok((path, false))
}

pub fn config_path() -> Result<PathBuf> {
    if let Some(path) = env::var_os("TOKENCHECK_CONFIG") {
        return Ok(PathBuf::from(path));
    }
    if let Some(dir) = env::var_os("XDG_CONFIG_HOME") {
        return Ok(PathBuf::from(dir).join("tokencheck").join("config.json"));
    }
    Ok(home_dir()?
        .join(".config")
        .join("tokencheck")
        .join("config.json"))
}

fn load_or_default_from(path: &Path) -> Result<AppConfig> {
    if !path.exists() {
        return Ok(AppConfig::default());
    }
    let content = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let mut config = serde_json::from_str::<AppConfig>(&content)
        .with_context(|| format!("parse {}", path.display()))?;
    if config.limit == 0 {
        config.limit = DEFAULT_LIMIT;
    }
    if config.heatmap_months == 0 {
        config.heatmap_months = DEFAULT_HEATMAP_MONTHS;
    }
    if config.data_file.as_os_str().is_empty() {
        config.data_file = PathBuf::from(DEFAULT_DATA_FILE);
    }
    Ok(config)
}

fn save_to_path(path: &Path, config: &AppConfig) -> Result<()> {
    if config.limit == 0 {
        return Err(anyhow!("config limit must be greater than 0"));
    }
    if config.heatmap_months == 0 {
        return Err(anyhow!("config heatmap_months must be greater than 0"));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let mut content = serde_json::to_string_pretty(config)?;
    content.push('\n');
    let tmp_path = path.with_extension("tmp");
    fs::write(&tmp_path, content).with_context(|| format!("write {}", tmp_path.display()))?;
    fs::rename(&tmp_path, path)
        .with_context(|| format!("replace {} with {}", path.display(), tmp_path.display()))?;
    Ok(())
}

fn home_dir() -> Result<PathBuf> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("HOME is not set; pass --home explicitly"))
}

#[cfg(test)]
mod tests {
    use super::{AppConfig, Language, SourcePreference, DEFAULT_DATA_FILE};

    #[test]
    fn defaults_are_stable() {
        let config = AppConfig::default();
        assert_eq!(config.language, Language::En);
        assert_eq!(config.source, SourcePreference::All);
        assert_eq!(config.data_file.to_string_lossy(), DEFAULT_DATA_FILE);
        assert_eq!(config.limit, 20);
        assert_eq!(config.heatmap_months, 12);
    }

    #[test]
    fn parses_language_aliases() {
        assert_eq!(Language::parse("en"), Some(Language::En));
        assert_eq!(Language::parse("中文"), Some(Language::Zh));
        assert_eq!(Language::parse("bad"), None);
    }

    #[test]
    fn parses_source_aliases() {
        assert_eq!(SourcePreference::parse("all"), Some(SourcePreference::All));
        assert_eq!(
            SourcePreference::parse("claude"),
            Some(SourcePreference::Claude)
        );
        assert_eq!(
            SourcePreference::parse("codex"),
            Some(SourcePreference::Codex)
        );
        assert_eq!(SourcePreference::parse("bad"), None);
    }

    #[test]
    fn missing_fields_use_project_defaults() {
        let config = serde_json::from_str::<AppConfig>(r#"{"language":"zh"}"#).unwrap();
        assert_eq!(config.language, Language::Zh);
        assert_eq!(config.source, SourcePreference::All);
        assert_eq!(config.data_file.to_string_lossy(), DEFAULT_DATA_FILE);
        assert_eq!(config.limit, 20);
        assert_eq!(config.heatmap_months, 12);
    }
}
