#![forbid(unsafe_code)]

pub mod billing;
pub mod claude_code;
pub mod codex;
pub mod config;
pub mod model;
pub mod opencode;
pub mod snapshot;

use crate::config::{AppConfig, Language, SourcePreference};
use crate::date::{
    days_in_month, month_abbr, parse_date_filter_value, today_utc, weekday_label, CivilDate,
};
use crate::model::{ReportData, Roots, Source, Usage};
use crate::render::{
    bold_yellow, center_visible, dim, print_rounded_panel, print_table, terminal_panel_width,
};
use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};

mod date;
mod render;

const HISTOGRAM_COLUMN_WIDTH: usize = 6;
const HISTOGRAM_HEIGHT: usize = 10;
const HEATMAP_LABEL_WIDTH: usize = 8;
const HEATMAP_WEEK_WIDTH: usize = 3;
const OBSIDIAN_DASHBOARD_TEMPLATE: &str = include_str!("../assets/obsidian-dashboard.md");

#[derive(Parser, Debug)]
#[command(name = "tokencheck")]
#[command(about = "Local Claude Code, Codex, and OpenCode usage stats")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    #[arg(long, global = true, value_enum, help = "Select data source")]
    source: Option<SourceFilter>,

    #[arg(long, global = true, help = "Override the home directory to scan")]
    home: Option<PathBuf>,

    #[arg(long, global = true, help = "Limit rows or chart columns")]
    limit: Option<usize>,

    #[arg(long, global = true, help = "Read only from the JSON snapshot")]
    from_json: bool,

    #[arg(long, global = true, help = "Read or write the JSON snapshot file")]
    data_file: Option<PathBuf>,

    #[arg(long, global = true, help = "Load custom model pricing JSON file")]
    pricing_file: Option<PathBuf>,

    #[arg(
        long,
        global = true,
        value_name = "DATE",
        help = "Only include report data on or after this date"
    )]
    since: Option<String>,

    #[arg(
        long,
        global = true,
        value_name = "DATE",
        help = "Only include report data on or before this date"
    )]
    until: Option<String>,
}

#[derive(Clone, Debug, Subcommand)]
enum Command {
    #[command(about = "Configure default language, source, snapshot, and display settings")]
    Config {
        #[command(subcommand)]
        command: Option<ConfigCommand>,
    },
    #[command(about = "Scan local logs and merge them into the JSON snapshot")]
    Fetch,
    #[command(about = "Show total usage, cost, and per-source totals")]
    Summary,
    #[command(about = "Show daily token and cost totals")]
    Days {
        #[arg(long, help = "Render a terminal histogram")]
        chant: bool,
    },
    #[command(about = "Show a terminal contribution heatmap")]
    Heatmap {
        #[arg(long, help = "Number of recent months to display")]
        months: Option<usize>,
    },
    #[command(about = "Diagnose config, snapshot, and local data availability")]
    Doctor,
    #[command(about = "Write an Obsidian Dataview dashboard from configured paths")]
    Obsidian {
        #[arg(
            long,
            help = "Scan and update the Obsidian snapshot before writing the dashboard"
        )]
        fetch: bool,

        #[arg(long, help = "Override the configured Obsidian snapshot JSON file")]
        snapshot_file: Option<PathBuf>,

        #[arg(
            long,
            help = "Override the configured Obsidian dashboard Markdown file"
        )]
        dashboard_file: Option<PathBuf>,
    },
    #[command(about = "Rank usage by project path")]
    Projects,
    #[command(about = "Rank usage and estimated cost by model")]
    Models,
    #[command(about = "Rank tool call counts by tool name")]
    Tools,
}

#[derive(Clone, Debug, Subcommand)]
enum ConfigCommand {
    #[command(about = "Show the effective configuration")]
    Show,
    #[command(about = "Delete the configuration file and use defaults")]
    Reset,
}

impl Command {
    fn shows_cost(&self) -> bool {
        matches!(
            self,
            Command::Summary | Command::Days { .. } | Command::Projects | Command::Models
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum SourceFilter {
    All,
    Claude,
    Codex,
    #[value(name = "opencode", alias = "open-code", alias = "oc")]
    OpenCode,
}

impl From<SourcePreference> for SourceFilter {
    fn from(value: SourcePreference) -> Self {
        match value {
            SourcePreference::All => SourceFilter::All,
            SourcePreference::Claude => SourceFilter::Claude,
            SourcePreference::Codex => SourceFilter::Codex,
            SourcePreference::OpenCode => SourceFilter::OpenCode,
        }
    }
}

impl From<SourceFilter> for SourcePreference {
    fn from(value: SourceFilter) -> Self {
        match value {
            SourceFilter::All => SourcePreference::All,
            SourceFilter::Claude => SourcePreference::Claude,
            SourceFilter::Codex => SourcePreference::Codex,
            SourceFilter::OpenCode => SourcePreference::OpenCode,
        }
    }
}

impl SourceFilter {
    fn as_str(self) -> &'static str {
        match self {
            SourceFilter::All => "all",
            SourceFilter::Claude => "claude",
            SourceFilter::Codex => "codex",
            SourceFilter::OpenCode => "opencode",
        }
    }
}

#[derive(Clone, Debug)]
struct EffectiveSettings {
    language: Language,
    source: SourceFilter,
    limit: usize,
    data_file: PathBuf,
    pricing_file: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DateFilter {
    since: Option<CivilDate>,
    until: Option<CivilDate>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DoctorRow {
    check: String,
    status: DoctorStatus,
    detail: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DoctorStatus {
    Ok,
    Warn,
}

impl DoctorStatus {
    fn label(self, language: Language) -> &'static str {
        match self {
            DoctorStatus::Ok => text(language, "ok", "正常"),
            DoctorStatus::Warn => text(language, "warn", "警告"),
        }
    }
}

impl DateFilter {
    fn parse(since: Option<&str>, until: Option<&str>) -> Result<Option<Self>> {
        if since.is_none() && until.is_none() {
            return Ok(None);
        }

        let today = today_utc()?;
        let since = since
            .map(|value| parse_date_filter_value(value, today))
            .transpose()?;
        let until = until
            .map(|value| parse_date_filter_value(value, today))
            .transpose()?;

        if let (Some(since), Some(until)) = (since, until) {
            if since > until {
                return Err(anyhow!(
                    "--since must be earlier than or equal to --until (got {since} > {until})"
                ));
            }
        }

        Ok(Some(Self { since, until }))
    }

    fn contains(self, date: &str) -> bool {
        let Some(date) = CivilDate::parse(date) else {
            return false;
        };
        if self.since.is_some_and(|since| date < since) {
            return false;
        }
        if self.until.is_some_and(|until| date > until) {
            return false;
        }
        true
    }
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();
    let command = cli.command.unwrap_or(Command::Summary);
    let app_config = config::load_or_default()?;
    let command = match command {
        Command::Config { command } => return run_config(app_config, command),
        command => command,
    };

    let settings = EffectiveSettings {
        language: app_config.language,
        source: cli.source.unwrap_or_else(|| app_config.source.into()),
        limit: cli.limit.unwrap_or(app_config.limit),
        data_file: expand_home_path(
            cli.data_file
                .unwrap_or_else(|| app_config.data_file.clone()),
        )?,
        pricing_file: cli
            .pricing_file
            .or_else(|| app_config.pricing_file.clone())
            .map(expand_home_path)
            .transpose()?,
    };
    let home = cli.home.clone();

    if matches!(command, Command::Fetch) {
        return run_fetch(
            settings.source,
            home,
            settings.data_file.clone(),
            settings.language,
        );
    }
    if matches!(command, Command::Doctor) {
        return run_doctor(
            settings.source,
            home,
            &settings.data_file,
            settings.pricing_file.as_deref(),
            settings.language,
        );
    }
    if let Command::Obsidian {
        fetch,
        snapshot_file,
        dashboard_file,
    } = &command
    {
        let snapshot_file = resolve_obsidian_snapshot_file(snapshot_file, &app_config, &settings)?;
        let dashboard_file = resolve_obsidian_dashboard_file(dashboard_file, &app_config)?;
        return run_obsidian(
            settings.source,
            home,
            snapshot_file,
            dashboard_file,
            *fetch,
            settings.language,
        );
    }

    let shows_cost = command.shows_cost();
    let pricing = if shows_cost {
        load_pricing(settings.pricing_file.as_deref())?
    } else {
        billing::Pricing::default()
    };
    let date_filter = DateFilter::parse(cli.since.as_deref(), cli.until.as_deref())?;
    let mut data = report_data(settings.source, home, &settings.data_file, cli.from_json)?;
    apply_date_filter(&mut data, date_filter);

    match command {
        Command::Config { .. } => unreachable!("config returns before report rendering"),
        Command::Fetch => unreachable!("fetch returns before report rendering"),
        Command::Summary => print_summary(&data, settings.language, &pricing),
        Command::Days { chant } => {
            print_days(&data, settings.limit, chant, settings.language, &pricing)
        }
        Command::Heatmap { months } => print_heatmap(
            &data,
            months.unwrap_or(app_config.heatmap_months),
            settings.language,
            &pricing,
        ),
        Command::Doctor => unreachable!("doctor returns before report rendering"),
        Command::Obsidian { .. } => unreachable!("obsidian returns before report rendering"),
        Command::Projects => print_projects(&data, settings.limit, settings.language, &pricing),
        Command::Models => print_models(&data, settings.limit, settings.language, &pricing),
        Command::Tools => print_tools(&data, settings.limit, settings.language),
    }

    let mut warnings = data.warnings.clone();
    if shows_cost {
        warnings.extend(localized_unpriced_model_warnings(
            settings.language,
            data.usage_events.iter(),
            &pricing,
        ));
    }
    print_warnings(settings.language, &warnings);

    Ok(())
}

fn run_config(app_config: AppConfig, command: Option<ConfigCommand>) -> Result<()> {
    match command {
        Some(ConfigCommand::Show) => print_config(app_config),
        Some(ConfigCommand::Reset) => reset_config(app_config.language),
        None => run_config_interactive(app_config),
    }
}

fn run_config_interactive(mut app_config: AppConfig) -> Result<()> {
    let config_path = config::config_path()?;
    println!("{}", label_config_title(app_config.language));
    println!(
        "{}: {}",
        label_config_file(app_config.language),
        config_path.display()
    );
    println!("{}", label_config_keep_hint(app_config.language));
    println!();

    app_config.language = prompt_language(app_config.language)?;
    app_config.source = prompt_source(app_config.language, app_config.source)?;
    app_config.data_file = prompt_path(
        app_config.language,
        label_config_data_file(app_config.language),
        &app_config.data_file,
    )?;
    app_config.pricing_file = prompt_optional_path(
        app_config.language,
        label_config_pricing_file(app_config.language),
        app_config.pricing_file.as_deref(),
    )?;
    app_config.obsidian_snapshot_file = prompt_optional_path(
        app_config.language,
        label_config_obsidian_snapshot_file(app_config.language),
        app_config.obsidian_snapshot_file.as_deref(),
    )?;
    app_config.obsidian_dashboard_file = prompt_optional_path(
        app_config.language,
        label_config_obsidian_dashboard_file(app_config.language),
        app_config.obsidian_dashboard_file.as_deref(),
    )?;
    app_config.limit = prompt_positive_usize(
        app_config.language,
        label_config_limit(app_config.language),
        app_config.limit,
    )?;
    app_config.heatmap_months = prompt_positive_usize(
        app_config.language,
        label_config_heatmap_months(app_config.language),
        app_config.heatmap_months,
    )?;

    let saved_path = config::save(&app_config)?;
    println!();
    println!(
        "{}: {}",
        label_config_saved(app_config.language),
        saved_path.display()
    );
    Ok(())
}

fn print_config(app_config: AppConfig) -> Result<()> {
    let config_path = config::config_path()?;
    let language = app_config.language;
    println!("{}", label_config_title(language));
    println!("{}: {}", label_config_file(language), config_path.display());
    println!(
        "{}: {}",
        label_config_status(language),
        if config_path.exists() {
            label_configured(language)
        } else {
            label_defaults(language)
        }
    );
    println!("{}: {}", label_config_language(language), language.as_str());
    println!(
        "{}: {}",
        label_config_source(language),
        app_config.source.as_str()
    );
    println!(
        "{}: {}",
        label_config_data_file(language),
        app_config.data_file.display()
    );
    println!(
        "{}: {}",
        label_config_pricing_file(language),
        format_optional_path(app_config.pricing_file.as_deref(), language)
    );
    println!(
        "{}: {}",
        label_config_obsidian_snapshot_file(language),
        format_optional_path(app_config.obsidian_snapshot_file.as_deref(), language)
    );
    println!(
        "{}: {}",
        label_config_obsidian_dashboard_file(language),
        format_optional_path(app_config.obsidian_dashboard_file.as_deref(), language)
    );
    println!("{}: {}", label_config_limit(language), app_config.limit);
    println!(
        "{}: {}",
        label_config_heatmap_months(language),
        app_config.heatmap_months
    );
    Ok(())
}

fn reset_config(language: Language) -> Result<()> {
    let (path, removed) = config::reset()?;
    if removed {
        println!("{}: {}", label_config_reset(language), path.display());
    } else {
        println!(
            "{}: {}",
            label_config_already_default(language),
            path.display()
        );
    }
    Ok(())
}

fn load_pricing(pricing_file: Option<&Path>) -> Result<billing::Pricing> {
    match pricing_file {
        Some(path) => billing::Pricing::load(path),
        None => Ok(billing::Pricing::default()),
    }
}

fn resolve_obsidian_snapshot_file(
    override_path: &Option<PathBuf>,
    app_config: &AppConfig,
    settings: &EffectiveSettings,
) -> Result<PathBuf> {
    let path = override_path
        .clone()
        .or_else(|| app_config.obsidian_snapshot_file.clone())
        .unwrap_or_else(|| settings.data_file.clone());
    expand_home_path(path)
}

fn resolve_obsidian_dashboard_file(
    override_path: &Option<PathBuf>,
    app_config: &AppConfig,
) -> Result<PathBuf> {
    let path = override_path
        .clone()
        .or_else(|| app_config.obsidian_dashboard_file.clone())
        .ok_or_else(|| anyhow!("Obsidian dashboard file is not configured; run tkc config"))?;
    expand_home_path(path)
}

fn run_obsidian(
    filter: SourceFilter,
    home: Option<PathBuf>,
    snapshot_file: PathBuf,
    dashboard_file: PathBuf,
    fetch: bool,
    language: Language,
) -> Result<()> {
    if fetch {
        run_fetch(filter, home, snapshot_file.clone(), language)?;
        println!();
    }

    write_obsidian_dashboard(&snapshot_file, &dashboard_file)?;
    println!(
        "{}: {}",
        label_obsidian_dashboard_saved(language),
        dashboard_file.display()
    );
    println!(
        "{}: {}",
        label_config_obsidian_snapshot_file(language),
        snapshot_file.display()
    );
    if !snapshot_file.exists() {
        eprintln!(
            "{}: {}",
            label_obsidian_snapshot_missing(language),
            snapshot_file.display()
        );
    }
    Ok(())
}

fn write_obsidian_dashboard(snapshot_file: &Path, dashboard_file: &Path) -> Result<()> {
    if let Some(parent) = dashboard_file.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let content = obsidian_dashboard_markdown(snapshot_file, dashboard_file);
    fs::write(dashboard_file, content)
        .with_context(|| format!("write {}", dashboard_file.display()))
}

fn obsidian_dashboard_markdown(snapshot_file: &Path, dashboard_file: &Path) -> String {
    let home_path = env::var_os("HOME").map(PathBuf::from);
    obsidian_dashboard_markdown_with_home(snapshot_file, dashboard_file, home_path.as_deref())
}

fn obsidian_dashboard_markdown_with_home(
    snapshot_file: &Path,
    dashboard_file: &Path,
    home_path: Option<&Path>,
) -> String {
    let snapshot_path = obsidian_dataview_snapshot_path(snapshot_file, dashboard_file);
    let home_path = home_path.map(path_to_slash_string).unwrap_or_default();
    OBSIDIAN_DASHBOARD_TEMPLATE
        .replace(
            "__TOKENCHECK_SNAPSHOT_PATH__",
            &escape_javascript_string(&snapshot_path),
        )
        .replace(
            "__TOKENCHECK_HOME_PATH__",
            &escape_javascript_string(&home_path),
        )
}

fn obsidian_dataview_snapshot_path(snapshot_file: &Path, dashboard_file: &Path) -> String {
    if snapshot_file.is_relative() {
        return path_to_slash_string(snapshot_file);
    }
    if let Some(vault_root) = obsidian_vault_root(dashboard_file) {
        if let Ok(relative) = snapshot_file.strip_prefix(vault_root) {
            return path_to_slash_string(relative);
        }
    }
    path_to_slash_string(snapshot_file)
}

fn obsidian_vault_root(dashboard_file: &Path) -> Option<PathBuf> {
    let mut current = dashboard_file.parent();
    while let Some(dir) = current {
        if dir.join(".obsidian").is_dir() {
            return Some(dir.to_path_buf());
        }
        current = dir.parent();
    }
    None
}

fn path_to_slash_string(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn escape_javascript_string(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(ch),
        }
    }
    out
}

fn run_doctor(
    filter: SourceFilter,
    home: Option<PathBuf>,
    data_file: &Path,
    pricing_file: Option<&Path>,
    language: Language,
) -> Result<()> {
    let home = home.unwrap_or(home_dir()?);
    let roots = Roots { home: home.clone() };
    let config_path = config::config_path()?;
    let mut rows = vec![
        doctor_row(
            label_config_file(language),
            true,
            describe_config_path(&config_path, language),
        ),
        doctor_row(
            label_home_directory(language),
            home.exists(),
            describe_path_status(&home, language),
        ),
        doctor_row(
            label_source_filter(language),
            true,
            filter.as_str().to_string(),
        ),
        doctor_pricing_row(pricing_file, language),
    ];

    if source_filter_includes(filter, Source::Claude) {
        let path = roots.claude_projects();
        rows.push(doctor_row(
            label_claude_data_dir(language),
            path.exists(),
            describe_path_status(&path, language),
        ));
    }
    if source_filter_includes(filter, Source::Codex) {
        let path = roots.codex_sessions();
        rows.push(doctor_row(
            label_codex_data_dir(language),
            path.exists(),
            describe_path_status(&path, language),
        ));
    }
    if source_filter_includes(filter, Source::OpenCode) {
        let path = roots.opencode_root();
        rows.push(doctor_row(
            label_opencode_data_dir(language),
            path.exists(),
            describe_path_status(&path, language),
        ));
    }

    let mut snapshot_has_usage = false;
    if data_file.exists() {
        match snapshot::load(data_file) {
            Ok(data) => {
                let counts = DataCounts::from(&filter_data(data, filter));
                snapshot_has_usage = counts.usage_events > 0;
                rows.push(doctor_row(
                    label_snapshot_file(language),
                    true,
                    format!(
                        "{} ({})",
                        data_file.display(),
                        describe_counts(counts, language)
                    ),
                ));
            }
            Err(err) => rows.push(doctor_row(
                label_snapshot_file(language),
                false,
                format!("{} ({err:#})", data_file.display()),
            )),
        }
    } else {
        rows.push(doctor_row(
            label_snapshot_file(language),
            false,
            describe_missing_path(data_file, language),
        ));
    }

    let live = collect_data(filter, &roots)?;
    let live_counts = DataCounts::from(&live);
    rows.push(doctor_row(
        label_live_scan(language),
        live_counts.usage_events > 0,
        describe_counts(live_counts, language),
    ));
    rows.push(doctor_row(
        label_report_ready(language),
        snapshot_has_usage || live_counts.usage_events > 0,
        if snapshot_has_usage || live_counts.usage_events > 0 {
            text(language, "usage data available", "已有可用用量数据").to_string()
        } else {
            label_no_usage_data(language).to_string()
        },
    ));

    print_doctor_rows(&rows, language);
    print_warnings(language, &live.warnings);
    Ok(())
}

fn doctor_row(check: &str, ok: bool, detail: String) -> DoctorRow {
    DoctorRow {
        check: check.to_string(),
        status: if ok {
            DoctorStatus::Ok
        } else {
            DoctorStatus::Warn
        },
        detail,
    }
}

fn doctor_pricing_row(pricing_file: Option<&Path>, language: Language) -> DoctorRow {
    match pricing_file {
        Some(path) => match billing::Pricing::load(path) {
            Ok(pricing) => doctor_row(
                label_config_pricing_file(language),
                true,
                format!(
                    "{} ({} {})",
                    path.display(),
                    pricing.custom_model_count(),
                    label_custom_models(language)
                ),
            ),
            Err(err) => doctor_row(
                label_config_pricing_file(language),
                false,
                format!("{} ({err:#})", path.display()),
            ),
        },
        None => doctor_row(
            label_config_pricing_file(language),
            true,
            text(language, "built-in pricing only", "仅使用内置价格").to_string(),
        ),
    }
}

fn source_filter_includes(filter: SourceFilter, source: Source) -> bool {
    matches!(
        (filter, source),
        (SourceFilter::All, _)
            | (SourceFilter::Claude, Source::Claude)
            | (SourceFilter::Codex, Source::Codex)
            | (SourceFilter::OpenCode, Source::OpenCode)
    )
}

fn describe_path_status(path: &Path, language: Language) -> String {
    if path.exists() {
        format!("{} ({})", path.display(), label_exists(language))
    } else {
        describe_missing_path(path, language)
    }
}

fn describe_config_path(path: &Path, language: Language) -> String {
    if path.exists() {
        describe_path_status(path, language)
    } else {
        format!(
            "{} ({})",
            path.display(),
            text(language, "missing, using defaults", "不存在，使用默认值")
        )
    }
}

fn describe_missing_path(path: &Path, language: Language) -> String {
    format!("{} ({})", path.display(), label_missing(language))
}

fn describe_counts(counts: DataCounts, language: Language) -> String {
    format!(
        "{} {}, {} {}, {} {}, {} {}",
        counts.sessions,
        label_sessions(language),
        counts.usage_events,
        label_usage_events(language),
        counts.tool_events,
        label_tool_calls(language),
        format_number(counts.total_tokens),
        label_total_tokens(language),
    )
}

fn print_doctor_rows(rows: &[DoctorRow], language: Language) {
    println!("{}", label_doctor_title(language));
    print_table(
        &[
            label_check(language),
            label_status(language),
            label_detail(language),
        ],
        &rows
            .iter()
            .map(|row| {
                vec![
                    row.check.clone(),
                    row.status.label(language).to_string(),
                    row.detail.clone(),
                ]
            })
            .collect::<Vec<_>>(),
    );
}

fn prompt_language(current: Language) -> Result<Language> {
    loop {
        let prompt = format!(
            "{} [en/zh] ({}: {}): ",
            label_config_language(current),
            label_current(current),
            current.as_str()
        );
        let input = prompt_line(&prompt)?;
        if input.trim().is_empty() {
            return Ok(current);
        }
        if let Some(language) = Language::parse(&input) {
            return Ok(language);
        }
        eprintln!("{}", label_invalid_language(current));
    }
}

fn prompt_source(language: Language, current: SourcePreference) -> Result<SourcePreference> {
    loop {
        let prompt = format!(
            "{} [all/claude/codex/opencode] ({}: {}): ",
            label_config_source(language),
            label_current(language),
            current.as_str()
        );
        let input = prompt_line(&prompt)?;
        if input.trim().is_empty() {
            return Ok(current);
        }
        if let Some(source) = SourcePreference::parse(&input) {
            return Ok(source);
        }
        eprintln!("{}", label_invalid_source(language));
    }
}

fn prompt_path(language: Language, label: &str, current: &Path) -> Result<PathBuf> {
    let prompt = format!(
        "{} ({}: {}): ",
        label,
        label_current(language),
        current.display()
    );
    let input = prompt_line(&prompt)?;
    if input.trim().is_empty() {
        Ok(current.to_path_buf())
    } else {
        expand_home_path(PathBuf::from(input.trim()))
    }
}

fn prompt_optional_path(
    language: Language,
    label: &str,
    current: Option<&Path>,
) -> Result<Option<PathBuf>> {
    let prompt = format!(
        "{} [path/none] ({}: {}): ",
        label,
        label_current(language),
        format_optional_path(current, language)
    );
    let input = prompt_line(&prompt)?;
    let input = input.trim();
    if input.is_empty() {
        return Ok(current.map(Path::to_path_buf));
    }
    if matches!(input.to_ascii_lowercase().as_str(), "none" | "null" | "-") {
        return Ok(None);
    }
    Ok(Some(expand_home_path(PathBuf::from(input))?))
}

fn format_optional_path(path: Option<&Path>, language: Language) -> String {
    path.map(|path| path.display().to_string())
        .unwrap_or_else(|| label_none(language).to_string())
}

fn prompt_positive_usize(language: Language, label: &str, current: usize) -> Result<usize> {
    loop {
        let prompt = format!("{} ({}: {}): ", label, label_current(language), current);
        let input = prompt_line(&prompt)?;
        if input.trim().is_empty() {
            return Ok(current);
        }
        if let Ok(value) = input.trim().parse::<usize>() {
            if value > 0 {
                return Ok(value);
            }
        }
        eprintln!("{}", label_invalid_positive_number(language));
    }
}

fn prompt_line(prompt: &str) -> Result<String> {
    print!("{prompt}");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim_end_matches(['\r', '\n']).to_string())
}

fn expand_home_path(path: PathBuf) -> Result<PathBuf> {
    let mut components = path.components();
    if let Some(Component::Normal(first)) = components.next() {
        if first == "~" {
            let mut expanded = home_dir()?;
            expanded.extend(components);
            return Ok(expanded);
        }
    }
    Ok(path)
}

fn print_warnings(language: Language, warnings: &[String]) {
    if warnings.is_empty() {
        return;
    }
    eprintln!("\n{}:", label_warnings(language));
    for warning in warnings {
        eprintln!("- {}", localized_warning(language, warning));
    }
}

fn localized_warning(language: Language, warning: &str) -> String {
    if matches!(language, Language::En) {
        return warning.to_string();
    }

    if let Some(path) = warning.strip_prefix("Claude Code data directory not found: ") {
        return format!("未找到 Claude Code 数据目录: {path}");
    }
    if let Some(path) = warning.strip_prefix("Codex data directory not found: ") {
        return format!("未找到 Codex 数据目录: {path}");
    }
    if let Some(path) = warning.strip_prefix("OpenCode data directory not found: ") {
        return format!("未找到 OpenCode 数据目录: {path}");
    }
    if let Some(err) = warning.strip_prefix("Failed to walk Claude Code data: ") {
        return format!("遍历 Claude Code 数据失败: {err}");
    }
    if let Some(err) = warning.strip_prefix("Failed to walk Codex data: ") {
        return format!("遍历 Codex 数据失败: {err}");
    }
    if let Some(err) = warning.strip_prefix("Failed to walk OpenCode data: ") {
        return format!("遍历 OpenCode 数据失败: {err}");
    }
    if let Some(err) = warning.strip_prefix("Failed to walk OpenCode session data: ") {
        return format!("遍历 OpenCode 会话数据失败: {err}");
    }
    if let Some(rest) = warning.strip_prefix("Failed to read Claude Code file ") {
        return format!("读取 Claude Code 文件失败: {rest}");
    }
    if let Some(rest) = warning.strip_prefix("Failed to read Codex file ") {
        return format!("读取 Codex 文件失败: {rest}");
    }
    if let Some(rest) = warning.strip_prefix("Failed to read OpenCode database ") {
        return format!("读取 OpenCode 数据库失败: {rest}");
    }
    if let Some(rest) = warning.strip_prefix("Failed to read OpenCode session file ") {
        return format!("读取 OpenCode 会话文件失败: {rest}");
    }
    if let Some(rest) = warning.strip_prefix("Failed to read OpenCode message file ") {
        return format!("读取 OpenCode 消息文件失败: {rest}");
    }
    if let Some(rest) = warning.strip_prefix("Invalid JSON in ") {
        return format!("无效 JSON: {rest}");
    }

    warning.to_string()
}

fn localized_unpriced_model_warnings<'a>(
    language: Language,
    events: impl Iterator<Item = &'a crate::model::UsageEvent>,
    pricing: &billing::Pricing,
) -> Vec<String> {
    pricing
        .unpriced_models(events)
        .into_iter()
        .map(|model| match language {
            Language::En => {
                format!("No pricing configured for {model}; omitted from dollar totals")
            }
            Language::Zh => format!("未配置 {model} 的价格；已从美元总额中忽略"),
        })
        .collect()
}

fn text(language: Language, en: &'static str, zh: &'static str) -> &'static str {
    match language {
        Language::En => en,
        Language::Zh => zh,
    }
}

fn label_config_title(language: Language) -> &'static str {
    text(language, "tokencheck config", "tokencheck 配置")
}

fn label_config_file(language: Language) -> &'static str {
    text(language, "Config file", "配置文件")
}

fn label_config_keep_hint(language: Language) -> &'static str {
    text(
        language,
        "Press Enter on a blank input to keep and save the shown value.",
        "直接按 Enter 会保留并保存当前显示的值。",
    )
}

fn label_config_saved(language: Language) -> &'static str {
    text(language, "config saved", "配置已保存")
}

fn label_config_reset(language: Language) -> &'static str {
    text(language, "config reset", "配置已重置")
}

fn label_config_already_default(language: Language) -> &'static str {
    text(language, "config already default", "配置已经是默认值")
}

fn label_obsidian_dashboard_saved(language: Language) -> &'static str {
    text(language, "Obsidian dashboard saved", "Obsidian 报表已保存")
}

fn label_obsidian_snapshot_missing(language: Language) -> &'static str {
    text(
        language,
        "Obsidian snapshot is missing; run tkc obsidian --fetch or tkc fetch",
        "Obsidian 快照不存在；请运行 tkc obsidian --fetch 或 tkc fetch",
    )
}

fn label_config_status(language: Language) -> &'static str {
    text(language, "Status", "状态")
}

fn label_configured(language: Language) -> &'static str {
    text(language, "configured", "已配置")
}

fn label_defaults(language: Language) -> &'static str {
    text(language, "defaults", "默认值")
}

fn label_config_language(language: Language) -> &'static str {
    text(language, "Language", "语言")
}

fn label_config_source(language: Language) -> &'static str {
    text(language, "Default source", "默认数据来源")
}

fn label_config_data_file(language: Language) -> &'static str {
    text(language, "Snapshot data file", "快照数据文件")
}

fn label_config_pricing_file(language: Language) -> &'static str {
    text(language, "Custom pricing file", "自定义价格文件")
}

fn label_config_obsidian_snapshot_file(language: Language) -> &'static str {
    text(language, "Obsidian snapshot file", "Obsidian 快照文件")
}

fn label_config_obsidian_dashboard_file(language: Language) -> &'static str {
    text(language, "Obsidian dashboard file", "Obsidian 报表文件")
}

fn label_config_limit(language: Language) -> &'static str {
    text(language, "Default row limit", "默认输出行数")
}

fn label_config_heatmap_months(language: Language) -> &'static str {
    text(language, "Default heatmap months", "默认热力图月份数")
}

fn label_doctor_title(language: Language) -> &'static str {
    text(language, "tokencheck doctor", "tokencheck 诊断")
}

fn label_check(language: Language) -> &'static str {
    text(language, "check", "检查项")
}

fn label_status(language: Language) -> &'static str {
    text(language, "status", "状态")
}

fn label_detail(language: Language) -> &'static str {
    text(language, "detail", "详情")
}

fn label_home_directory(language: Language) -> &'static str {
    text(language, "Home directory", "用户目录")
}

fn label_source_filter(language: Language) -> &'static str {
    text(language, "Source filter", "数据来源过滤")
}

fn label_claude_data_dir(language: Language) -> &'static str {
    text(
        language,
        "Claude Code data directory",
        "Claude Code 数据目录",
    )
}

fn label_codex_data_dir(language: Language) -> &'static str {
    text(language, "Codex data directory", "Codex 数据目录")
}

fn label_opencode_data_dir(language: Language) -> &'static str {
    text(language, "OpenCode data directory", "OpenCode 数据目录")
}

fn label_snapshot_file(language: Language) -> &'static str {
    text(language, "Snapshot file", "快照文件")
}

fn label_live_scan(language: Language) -> &'static str {
    text(language, "Live scan", "实时扫描")
}

fn label_report_ready(language: Language) -> &'static str {
    text(language, "Report data", "报表数据")
}

fn label_exists(language: Language) -> &'static str {
    text(language, "exists", "存在")
}

fn label_none(language: Language) -> &'static str {
    text(language, "none", "无")
}

fn label_custom_models(language: Language) -> &'static str {
    text(language, "custom models", "个自定义模型")
}

fn label_missing(language: Language) -> &'static str {
    text(language, "missing", "不存在")
}

fn label_current(language: Language) -> &'static str {
    text(language, "current", "当前")
}

fn label_invalid_language(language: Language) -> &'static str {
    text(
        language,
        "Invalid language. Use en or zh.",
        "无效语言。请输入 en 或 zh。",
    )
}

fn label_invalid_source(language: Language) -> &'static str {
    text(
        language,
        "Invalid source. Use all, claude, codex, or opencode.",
        "无效数据来源。请输入 all、claude、codex 或 opencode。",
    )
}

fn label_invalid_positive_number(language: Language) -> &'static str {
    text(
        language,
        "Invalid number. Enter a positive integer.",
        "无效数字。请输入正整数。",
    )
}

fn label_snapshot_saved(language: Language) -> &'static str {
    text(language, "snapshot saved", "快照已保存")
}

fn label_snapshot_unchanged(language: Language) -> &'static str {
    text(language, "snapshot unchanged", "快照未变化")
}

fn label_warnings(language: Language) -> &'static str {
    text(language, "Warnings", "警告")
}

fn label_summary_title(language: Language) -> &'static str {
    text(language, "tokencheck summary", "tokencheck 总览")
}

fn label_sessions_scanned(language: Language) -> &'static str {
    text(language, "sessions scanned", "扫描会话数")
}

fn label_sessions_with_usage(language: Language) -> &'static str {
    text(language, "sessions with usage", "包含用量的会话数")
}

fn label_projects_seen(language: Language) -> &'static str {
    text(language, "projects seen", "项目数")
}

fn label_models_seen(language: Language) -> &'static str {
    text(language, "models seen", "模型数")
}

fn label_estimated_cost(language: Language) -> &'static str {
    text(language, "estimated cost", "估算成本")
}

fn label_sessions(language: Language) -> &'static str {
    text(language, "sessions", "会话")
}

fn label_usage_events(language: Language) -> &'static str {
    text(language, "usage events", "用量事件")
}

fn label_usage(language: Language) -> &'static str {
    text(language, "usage", "用量")
}

fn label_tools(language: Language) -> &'static str {
    text(language, "tools", "工具")
}

fn label_tool_calls(language: Language) -> &'static str {
    text(language, "tool calls", "工具调用")
}

fn label_total_tokens(language: Language) -> &'static str {
    text(language, "total tokens", "总 token")
}

fn label_source(language: Language) -> &'static str {
    text(language, "source", "来源")
}

fn label_input(language: Language) -> &'static str {
    text(language, "input", "输入")
}

fn label_cached(language: Language) -> &'static str {
    text(language, "cached", "缓存")
}

fn label_cache_create(language: Language) -> &'static str {
    text(language, "cache_create", "写缓存")
}

fn label_output(language: Language) -> &'static str {
    text(language, "output", "输出")
}

fn label_reasoning(language: Language) -> &'static str {
    text(language, "reasoning", "推理")
}

fn label_total(language: Language) -> &'static str {
    text(language, "total", "总计")
}

fn label_cost(language: Language) -> &'static str {
    text(language, "cost", "成本")
}

fn label_cost_title(language: Language) -> &'static str {
    text(language, "Cost", "成本")
}

fn label_date(language: Language) -> &'static str {
    text(language, "date", "日期")
}

fn label_project(language: Language) -> &'static str {
    text(language, "project", "项目")
}

fn label_projects(language: Language) -> &'static str {
    text(language, "projects", "项目数")
}

fn label_model(language: Language) -> &'static str {
    text(language, "model", "模型")
}

fn label_tool(language: Language) -> &'static str {
    text(language, "tool", "工具")
}

fn label_calls(language: Language) -> &'static str {
    text(language, "calls", "调用")
}

fn label_days(language: Language) -> &'static str {
    text(language, "Days", "天数")
}

fn label_daily_usage(language: Language) -> &'static str {
    text(language, "Daily Usage", "每日用量")
}

fn label_contribution_heatmap(language: Language) -> &'static str {
    text(language, "Contribution Heatmap", "用量热力图")
}

fn label_terminal_too_narrow_chart(language: Language) -> &'static str {
    text(
        language,
        "Terminal is too narrow for the chart",
        "终端太窄，无法显示图表",
    )
}

fn label_terminal_too_narrow_heatmap(language: Language) -> &'static str {
    text(
        language,
        "Terminal is too narrow for the heatmap",
        "终端太窄，无法显示热力图",
    )
}

fn label_no_usage_data(language: Language) -> &'static str {
    text(language, "No usage data.", "没有可用用量数据。")
}

fn label_max(language: Language) -> &'static str {
    text(language, "Max", "最大")
}

fn label_tokens(language: Language) -> &'static str {
    text(language, "Tokens", "Token")
}

fn label_less(language: Language) -> &'static str {
    text(language, "Less", "低")
}

fn label_more(language: Language) -> &'static str {
    text(language, "More", "高")
}

fn label_upgraded(language: Language) -> &'static str {
    text(language, "upgraded", "已升级")
}

fn collect_data(filter: SourceFilter, roots: &Roots) -> Result<ReportData> {
    let mut data = ReportData::default();
    if matches!(filter, SourceFilter::All | SourceFilter::Claude) {
        data.merge(claude_code::collect(&roots.claude_projects())?);
    }
    if matches!(filter, SourceFilter::All | SourceFilter::Codex) {
        data.merge(codex::collect(&roots.codex_sessions())?);
    }
    if matches!(filter, SourceFilter::All | SourceFilter::OpenCode) {
        data.merge(opencode::collect(&roots.opencode_root())?);
    }
    Ok(data)
}

fn report_data(
    filter: SourceFilter,
    home: Option<PathBuf>,
    data_file: &Path,
    from_json: bool,
) -> Result<ReportData> {
    if from_json {
        return Ok(filter_data(snapshot::load(data_file)?, filter));
    }

    let roots = Roots {
        home: home.unwrap_or(home_dir()?),
    };
    let live = collect_data(filter, &roots)?;
    if !data_file.exists() {
        return Ok(live);
    }

    let warnings = live.warnings.clone();
    let mut data = snapshot::load(data_file)?;
    snapshot::merge_preserving_growth(&mut data, live);
    data.warnings = warnings;
    Ok(filter_data(data, filter))
}

fn run_fetch(
    filter: SourceFilter,
    home: Option<PathBuf>,
    data_file: PathBuf,
    language: Language,
) -> Result<()> {
    let roots = Roots {
        home: home.unwrap_or(home_dir()?),
    };
    let incoming = collect_data(filter, &roots)?;
    let warnings = incoming.warnings.clone();
    let file_exists = data_file.exists();
    let mut existing = snapshot::load_or_default(&data_file)?;
    let before = DataCounts::from(&existing);
    let summary = snapshot::merge_preserving_growth(&mut existing, incoming);
    let after = DataCounts::from(&existing);

    if summary.changed() || !file_exists {
        snapshot::save(&data_file, &existing)?;
        println!(
            "{}: {}",
            label_snapshot_saved(language),
            data_file.display()
        );
    } else {
        println!(
            "{}: {}",
            label_snapshot_unchanged(language),
            data_file.display()
        );
    }
    println!(
        "{}: {} -> {}",
        label_sessions(language),
        before.sessions,
        after.sessions
    );
    println!(
        "{}: {} -> {} (+{}, {} {})",
        label_usage_events(language),
        before.usage_events,
        after.usage_events,
        summary.usage_events_added,
        label_upgraded(language),
        summary.usage_events_upgraded
    );
    println!(
        "{}: {} -> {} (+{})",
        label_tool_calls(language),
        before.tool_events,
        after.tool_events,
        summary.tool_events_added
    );
    println!(
        "{}: {} -> {}",
        label_total_tokens(language),
        format_number(before.total_tokens),
        format_number(after.total_tokens)
    );

    print_warnings(language, &warnings);

    Ok(())
}

fn filter_data(mut data: ReportData, filter: SourceFilter) -> ReportData {
    if matches!(filter, SourceFilter::All) {
        return data;
    }
    data.sessions
        .retain(|session| source_matches(session.source, filter));
    data.usage_events
        .retain(|event| source_matches(event.source, filter));
    data.tool_events
        .retain(|event| source_matches(event.source, filter));
    data
}

fn apply_date_filter(data: &mut ReportData, filter: Option<DateFilter>) {
    let Some(filter) = filter else {
        return;
    };
    data.sessions
        .retain(|session| filter.contains(&session.date));
    data.usage_events
        .retain(|event| filter.contains(&event.date));
    data.tool_events
        .retain(|event| filter.contains(&event.date));
}

fn source_matches(source: Source, filter: SourceFilter) -> bool {
    matches!(
        (source, filter),
        (_, SourceFilter::All)
            | (Source::Claude, SourceFilter::Claude)
            | (Source::Codex, SourceFilter::Codex)
            | (Source::OpenCode, SourceFilter::OpenCode)
    )
}

#[derive(Clone, Copy, Debug, Default)]
struct DataCounts {
    sessions: usize,
    usage_events: usize,
    tool_events: usize,
    total_tokens: u64,
}

impl From<&ReportData> for DataCounts {
    fn from(data: &ReportData) -> Self {
        Self {
            sessions: data.sessions.len(),
            usage_events: data.usage_events.len(),
            tool_events: data.tool_events.len(),
            total_tokens: data
                .usage_events
                .iter()
                .map(|event| event.usage.computed_total())
                .sum(),
        }
    }
}

fn print_summary(data: &ReportData, language: Language, pricing: &billing::Pricing) {
    let mut rows = Vec::new();
    let sources = [Source::Claude, Source::Codex, Source::OpenCode];
    for source in sources {
        let session_count = unique_sessions(data, Some(source));
        let usage_events = data
            .usage_events
            .iter()
            .filter(|event| event.source == source)
            .count();
        let tool_calls = data
            .tool_events
            .iter()
            .filter(|event| event.source == source)
            .count();
        if session_count == 0 && usage_events == 0 && tool_calls == 0 {
            continue;
        }
        let stats = sum_usage_stats(
            data.usage_events
                .iter()
                .filter(|event| event.source == source),
            pricing,
        );
        rows.push(vec![
            source.label().to_string(),
            session_count.to_string(),
            usage_events.to_string(),
            tool_calls.to_string(),
            format_number(stats.usage.input),
            format_number(stats.usage.cached_input),
            format_number(stats.usage.cache_creation_input),
            format_number(stats.usage.output),
            format_number(stats.usage.reasoning_output),
            format_number(stats.usage.computed_total()),
            format_cost(stats.cost),
        ]);
    }

    let total_stats = sum_usage_stats(data.usage_events.iter(), pricing);
    let sessions_with_usage = data
        .usage_events
        .iter()
        .map(|event| format!("{}:{}", event.source.label(), event.session_id))
        .collect::<BTreeSet<_>>()
        .len();
    let projects_seen = data
        .sessions
        .iter()
        .map(|session| session.project.as_str())
        .filter(|project| *project != "unknown")
        .collect::<BTreeSet<_>>()
        .len();
    let models_seen = data
        .sessions
        .iter()
        .map(|session| session.model.as_str())
        .filter(|model| *model != "unknown")
        .collect::<BTreeSet<_>>()
        .len();
    println!("{}", label_summary_title(language));
    println!(
        "{}: {}",
        label_sessions_scanned(language),
        unique_sessions(data, None)
    );
    println!(
        "{}: {sessions_with_usage}",
        label_sessions_with_usage(language)
    );
    println!("{}: {projects_seen}", label_projects_seen(language));
    println!("{}: {models_seen}", label_models_seen(language));
    println!(
        "{}: {}",
        label_usage_events(language),
        data.usage_events.len()
    );
    println!("{}: {}", label_tool_calls(language), data.tool_events.len());
    println!(
        "{}: {}",
        label_total_tokens(language),
        format_number(total_stats.usage.computed_total())
    );
    println!(
        "{}: {}",
        label_estimated_cost(language),
        format_cost(total_stats.cost)
    );
    println!();
    print_table(
        &[
            label_source(language),
            label_sessions(language),
            label_usage(language),
            label_tools(language),
            label_input(language),
            label_cached(language),
            label_cache_create(language),
            label_output(language),
            label_reasoning(language),
            label_total(language),
            label_cost(language),
        ],
        &rows,
    );
}

fn print_days(
    data: &ReportData,
    limit: usize,
    chant: bool,
    language: Language,
    pricing: &billing::Pricing,
) {
    let rows = daily_usage_rows(data, limit, pricing);
    if chant {
        print_days_histogram(&rows, language);
        return;
    }

    let rows = rows
        .into_iter()
        .map(|row| {
            vec![
                row.date,
                row.sessions.to_string(),
                format_number(row.stats.usage.input),
                format_number(row.stats.usage.cached_input),
                format_number(row.stats.usage.cache_creation_input),
                format_number(row.stats.usage.output),
                format_number(row.stats.usage.reasoning_output),
                format_number(row.stats.usage.computed_total()),
                format_cost(row.stats.cost),
            ]
        })
        .collect::<Vec<_>>();

    print_table(
        &[
            label_date(language),
            label_sessions(language),
            label_input(language),
            label_cached(language),
            label_cache_create(language),
            label_output(language),
            label_reasoning(language),
            label_total(language),
            label_cost(language),
        ],
        &rows,
    );
}

fn daily_usage_rows(
    data: &ReportData,
    limit: usize,
    pricing: &billing::Pricing,
) -> Vec<DailyUsageRow> {
    let mut usage_by_date: BTreeMap<String, UsageStats> = BTreeMap::new();
    let mut sessions_by_date: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for event in &data.usage_events {
        usage_by_date
            .entry(event.date.clone())
            .or_default()
            .add(event, pricing);
    }
    for session in &data.sessions {
        sessions_by_date
            .entry(session.date.clone())
            .or_default()
            .insert(format!("{}:{}", session.source.label(), session.session_id));
    }

    usage_by_date
        .into_iter()
        .rev()
        .take(limit)
        .map(|(date, stats)| DailyUsageRow {
            sessions: sessions_by_date.get(&date).map(BTreeSet::len).unwrap_or(0),
            date,
            stats,
        })
        .collect()
}

fn print_days_histogram(rows: &[DailyUsageRow], language: Language) {
    if rows.is_empty() {
        println!("{}", label_no_usage_data(language));
        return;
    }

    let panel_width = terminal_panel_width();
    let inner_width = panel_width.saturating_sub(4);
    let visible_rows = visible_histogram_rows(rows, inner_width);
    if visible_rows.is_empty() {
        print_rounded_panel(
            label_daily_usage(language),
            &[dim(label_terminal_too_narrow_chart(language))],
        );
        return;
    }

    let max_usage = visible_rows
        .iter()
        .map(|row| row.stats.usage.computed_total())
        .max()
        .unwrap_or(0);
    let total_tokens = visible_rows
        .iter()
        .map(|row| row.stats.usage.computed_total())
        .sum();
    let mut total_cost = billing::Cost::default();
    for row in &visible_rows {
        total_cost.add_assign(row.stats.cost);
    }
    let days_label = if visible_rows.len() == rows.len() {
        visible_rows.len().to_string()
    } else {
        format!("{}/{}", visible_rows.len(), rows.len())
    };

    let mut lines = Vec::new();
    lines.push(format!(
        "{} {}   {} {}",
        dim(label_max(language)),
        bold_yellow(&format_number(max_usage)),
        dim(label_days(language)),
        bold_yellow(&days_label)
    ));
    lines.push(format!(
        "{} {}   {} {}",
        dim(label_tokens(language)),
        bold_yellow(&format_number(total_tokens)),
        dim(label_cost_title(language)),
        bold_yellow(&format_histogram_cost(total_cost))
    ));
    lines.push(String::new());
    lines.extend(vertical_histogram_lines(
        &visible_rows,
        max_usage,
        HISTOGRAM_HEIGHT,
    ));
    lines.push(String::new());
    lines.push(heatmap_legend_line(language));

    print_rounded_panel(label_daily_usage(language), &lines);
}

fn visible_histogram_rows(rows: &[DailyUsageRow], available_width: usize) -> Vec<DailyUsageRow> {
    let max_usage = rows
        .iter()
        .map(|row| row.stats.usage.computed_total())
        .max()
        .unwrap_or(0);
    let axis_width = format_number(max_usage).len().max(4);
    let max_columns = max_histogram_columns(available_width, axis_width).min(rows.len());
    rows.iter().take(max_columns).cloned().collect()
}

fn max_histogram_columns(available_width: usize, axis_width: usize) -> usize {
    available_width.saturating_sub(axis_width + 2) / HISTOGRAM_COLUMN_WIDTH
}

fn vertical_histogram_lines(rows: &[DailyUsageRow], max_usage: u64, height: usize) -> Vec<String> {
    let columns = rows
        .iter()
        .rev()
        .map(|row| {
            let total = row.stats.usage.computed_total();
            HistogramColumn {
                date: row.date.as_str(),
                total,
                cost: row.stats.cost,
                height: histogram_height(total, max_usage, height),
                level: heatmap_level(total, max_usage),
            }
        })
        .collect::<Vec<_>>();

    let axis_width = format_number(max_usage).len().max(4);
    let mut lines = Vec::new();
    for level in (1..=height).rev() {
        let label = histogram_axis_label(level, height, max_usage);
        let mut line = format!("{label:>axis_width$} │");
        for column in &columns {
            if column.height >= level {
                line.push_str(&center_visible(
                    &heatmap_cell_for_level(column.level),
                    HISTOGRAM_COLUMN_WIDTH,
                ));
            } else {
                line.push_str(&" ".repeat(HISTOGRAM_COLUMN_WIDTH));
            }
        }
        lines.push(line);
    }
    lines.push(format!(
        "{:>axis_width$} └{}",
        "",
        "─".repeat(columns.len() * HISTOGRAM_COLUMN_WIDTH)
    ));
    lines.push(histogram_label_row(&columns, axis_width, |column| {
        column.date.get(5..10).unwrap_or(column.date).to_string()
    }));
    lines.push(histogram_label_row(&columns, axis_width, |column| {
        format_number(column.total)
    }));
    lines.push(histogram_label_row(&columns, axis_width, |column| {
        format_histogram_cost(column.cost)
    }));
    lines
}

fn histogram_axis_label(level: usize, height: usize, max_usage: u64) -> String {
    if level == height {
        format_number(max_usage)
    } else if level == 1 {
        String::from("0")
    } else {
        String::new()
    }
}

fn print_heatmap(data: &ReportData, months: usize, language: Language, pricing: &billing::Pricing) {
    let mut usage_by_date: BTreeMap<CivilDate, UsageStats> = BTreeMap::new();
    for event in &data.usage_events {
        let Some(date) = CivilDate::parse(&event.date) else {
            continue;
        };
        usage_by_date.entry(date).or_default().add(event, pricing);
    }

    let Some(latest_date) = usage_by_date.keys().next_back().copied() else {
        println!("{}", label_no_usage_data(language));
        return;
    };

    let months = months.max(1).min(i32::MAX as usize) as i32;
    let latest_month = latest_date.month_index();
    let start_month = latest_month.saturating_sub(months - 1);
    let (start_year, start_month_number) = CivilDate::from_month_index(start_month);
    let (end_year, end_month_number) = CivilDate::from_month_index(latest_month);
    let start_date = CivilDate {
        year: start_year,
        month: start_month_number,
        day: 1,
    };
    let end_date = CivilDate {
        year: end_year,
        month: end_month_number,
        day: days_in_month(end_year, end_month_number),
    };
    let first_grid_date = start_date.add_days(-(start_date.weekday_sunday_index() as i64));
    let last_grid_date = end_date.add_days((6 - end_date.weekday_sunday_index()) as i64);
    let week_count =
        ((last_grid_date.days_since_epoch() - first_grid_date.days_since_epoch()) / 7 + 1) as usize;
    let panel_width = terminal_panel_width();
    let inner_width = panel_width.saturating_sub(4);
    let visible_week_count = max_heatmap_weeks(inner_width).min(week_count);
    if visible_week_count == 0 {
        print_rounded_panel(
            label_contribution_heatmap(language),
            &[dim(label_terminal_too_narrow_heatmap(language))],
        );
        return;
    }
    let skipped_weeks = week_count - visible_week_count;
    let visible_first_grid_date = first_grid_date.add_days((skipped_weeks * 7) as i64);
    let visible_start_date = visible_first_grid_date.max(start_date);
    let selected_max = usage_by_date
        .iter()
        .filter(|(date, _)| **date >= visible_start_date && **date <= end_date)
        .map(|(_, stats)| stats.usage.computed_total())
        .max()
        .unwrap_or(0);
    let range_label = if visible_week_count == week_count {
        format!("{months}mo")
    } else {
        format!("{visible_week_count}/{week_count}w")
    };

    let mut lines = Vec::new();
    lines.push(format!(
        "{} {}   {}",
        dim(label_max(language)),
        bold_yellow(&format_number(selected_max)),
        bold_yellow(&range_label)
    ));
    lines.push(String::new());
    lines.push(github_month_labels_line(
        visible_first_grid_date,
        visible_week_count,
        start_month,
        latest_month,
        start_date,
        end_date,
    ));
    lines.extend(github_heatmap_grid_lines(
        visible_first_grid_date,
        visible_week_count,
        start_date,
        end_date,
        selected_max,
        &usage_by_date,
    ));
    lines.push(String::new());
    lines.push(heatmap_legend_line(language));

    print_rounded_panel(label_contribution_heatmap(language), &lines);
}

fn max_heatmap_weeks(available_width: usize) -> usize {
    available_width.saturating_sub(HEATMAP_LABEL_WIDTH) / HEATMAP_WEEK_WIDTH
}

fn github_month_labels_line(
    first_grid_date: CivilDate,
    week_count: usize,
    start_month: i32,
    latest_month: i32,
    start_date: CivilDate,
    end_date: CivilDate,
) -> String {
    let mut labels = vec![String::from("   "); week_count];
    let first_visible_date = (0..week_count * 7)
        .map(|offset| first_grid_date.add_days(offset as i64))
        .find(|date| *date >= start_date && *date <= end_date);
    if let Some(date) = first_visible_date {
        labels[0] = month_abbr(date.month).to_string();
    }
    for month_index in start_month..=latest_month {
        let (year, month) = CivilDate::from_month_index(month_index);
        let first_of_month = CivilDate {
            year,
            month,
            day: 1,
        };
        let diff_days = first_of_month.days_since_epoch() - first_grid_date.days_since_epoch();
        if diff_days >= 0 {
            let week_index = (diff_days / 7) as usize;
            if week_index < labels.len() {
                place_month_label(&mut labels, week_index, month_abbr(month));
            }
        }
    }
    dim(&format!("        {}", labels.join("")))
}

fn place_month_label(labels: &mut [String], week_index: usize, label: &str) {
    if week_index > 0 && !labels[week_index - 1].trim().is_empty() {
        return;
    }
    labels[week_index] = label.to_string();
}

fn github_heatmap_grid_lines(
    first_grid_date: CivilDate,
    week_count: usize,
    start_date: CivilDate,
    end_date: CivilDate,
    max_usage: u64,
    usage_by_date: &BTreeMap<CivilDate, UsageStats>,
) -> Vec<String> {
    let mut lines = Vec::new();
    for weekday in 0..7 {
        let mut line = format!("{}  ", dim(&format!("{:>6}", weekday_label(weekday))));
        for week in 0..week_count {
            let date = first_grid_date.add_days((week * 7 + weekday) as i64);
            if date < start_date || date > end_date {
                line.push_str("   ");
                continue;
            }
            let usage = usage_by_date
                .get(&date)
                .map(|stats| stats.usage.computed_total())
                .unwrap_or(0);
            line.push_str(&heatmap_cell(usage, max_usage));
            line.push(' ');
        }
        lines.push(line);
    }
    lines
}

fn print_projects(data: &ReportData, limit: usize, language: Language, pricing: &billing::Pricing) {
    let rows = aggregate_usage(
        data,
        |event| (event.source.label().to_string(), event.project.clone()),
        limit,
        pricing,
    )
    .into_iter()
    .map(|((source, project), stats)| {
        vec![
            source,
            project,
            format_number(stats.usage.input),
            format_number(stats.usage.cached_input),
            format_number(stats.usage.cache_creation_input),
            format_number(stats.usage.output),
            format_number(stats.usage.reasoning_output),
            format_number(stats.usage.computed_total()),
            format_cost(stats.cost),
        ]
    })
    .collect::<Vec<_>>();

    print_table(
        &[
            label_source(language),
            label_project(language),
            label_input(language),
            label_cached(language),
            label_cache_create(language),
            label_output(language),
            label_reasoning(language),
            label_total(language),
            label_cost(language),
        ],
        &rows,
    );
}

fn print_models(data: &ReportData, limit: usize, language: Language, pricing: &billing::Pricing) {
    let rows = aggregate_usage(
        data,
        |event| (event.source.label().to_string(), event.model.clone()),
        limit,
        pricing,
    )
    .into_iter()
    .map(|((source, model), stats)| {
        vec![
            source,
            model,
            format_number(stats.usage.input),
            format_number(stats.usage.cached_input),
            format_number(stats.usage.cache_creation_input),
            format_number(stats.usage.output),
            format_number(stats.usage.reasoning_output),
            format_number(stats.usage.computed_total()),
            format_cost(stats.cost),
        ]
    })
    .collect::<Vec<_>>();

    print_table(
        &[
            label_source(language),
            label_model(language),
            label_input(language),
            label_cached(language),
            label_cache_create(language),
            label_output(language),
            label_reasoning(language),
            label_total(language),
            label_cost(language),
        ],
        &rows,
    );
}

fn print_tools(data: &ReportData, limit: usize, language: Language) {
    #[derive(Default)]
    struct ToolStats {
        calls: u64,
        days: BTreeSet<String>,
        projects: BTreeSet<String>,
    }

    let mut counts: BTreeMap<(String, String), ToolStats> = BTreeMap::new();
    for event in &data.tool_events {
        let stats = counts
            .entry((event.source.label().to_string(), event.tool.clone()))
            .or_default();
        stats.calls += 1;
        stats.days.insert(event.date.clone());
        stats.projects.insert(event.project.clone());
    }

    let mut rows = counts.into_iter().collect::<Vec<_>>();
    rows.sort_by(|a, b| b.1.calls.cmp(&a.1.calls).then_with(|| a.0.cmp(&b.0)));
    let rows = rows
        .into_iter()
        .take(limit)
        .map(|((source, tool), stats)| {
            vec![
                source,
                tool,
                format_number(stats.calls),
                stats.days.len().to_string(),
                stats.projects.len().to_string(),
            ]
        })
        .collect::<Vec<_>>();

    print_table(
        &[
            label_source(language),
            label_tool(language),
            label_calls(language),
            label_days(language),
            label_projects(language),
        ],
        &rows,
    );
}

fn aggregate_usage<K, F>(
    data: &ReportData,
    key_fn: F,
    limit: usize,
    pricing: &billing::Pricing,
) -> Vec<(K, UsageStats)>
where
    K: Ord + Clone,
    F: Fn(&crate::model::UsageEvent) -> K,
{
    let mut map: BTreeMap<K, UsageStats> = BTreeMap::new();
    for event in &data.usage_events {
        map.entry(key_fn(event)).or_default().add(event, pricing);
    }
    let mut rows = map.into_iter().collect::<Vec<_>>();
    rows.sort_by(|a, b| {
        b.1.usage
            .computed_total()
            .cmp(&a.1.usage.computed_total())
            .then_with(|| a.0.cmp(&b.0))
    });
    rows.truncate(limit);
    rows
}

#[derive(Clone, Debug, Default)]
struct UsageStats {
    usage: Usage,
    cost: billing::Cost,
}

#[derive(Clone, Debug)]
struct DailyUsageRow {
    date: String,
    sessions: usize,
    stats: UsageStats,
}

#[derive(Clone, Copy, Debug)]
struct HistogramColumn<'a> {
    date: &'a str,
    total: u64,
    cost: billing::Cost,
    height: usize,
    level: usize,
}

impl UsageStats {
    fn add(&mut self, event: &crate::model::UsageEvent, pricing: &billing::Pricing) {
        self.usage.add_assign(&event.usage);
        self.cost.add_assign(pricing.event_cost(event));
    }
}

fn sum_usage_stats<'a>(
    events: impl Iterator<Item = &'a crate::model::UsageEvent>,
    pricing: &billing::Pricing,
) -> UsageStats {
    let mut stats = UsageStats::default();
    for event in events {
        stats.add(event, pricing);
    }
    stats
}

fn unique_sessions(data: &ReportData, source: Option<Source>) -> usize {
    data.sessions
        .iter()
        .filter(|session| source.is_none_or(|source| session.source == source))
        .map(|session| format!("{}:{}", session.source.label(), session.session_id))
        .collect::<BTreeSet<_>>()
        .len()
}

fn histogram_label_row<'a>(
    columns: &[HistogramColumn<'a>],
    axis_width: usize,
    label_fn: impl Fn(&HistogramColumn<'a>) -> String,
) -> String {
    let mut line = format!("{:>axis_width$}  ", "");
    for column in columns {
        line.push_str(&center_visible(
            &fit_histogram_label(&label_fn(column), HISTOGRAM_COLUMN_WIDTH),
            HISTOGRAM_COLUMN_WIDTH,
        ));
    }
    line
}

fn fit_histogram_label(value: &str, width: usize) -> String {
    let chars = value.chars().collect::<Vec<_>>();
    if chars.len() <= width {
        return value.to_string();
    }
    chars.into_iter().take(width).collect()
}

fn format_histogram_cost(cost: billing::Cost) -> String {
    let marked = cost.unpriced_events > 0;
    let mut out = if cost.usd >= 100.0 {
        format!("${:.0}", cost.usd)
    } else if cost.usd >= 10.0 || marked {
        format!("${:.1}", cost.usd)
    } else {
        format_dollars(cost.usd)
    };
    if marked {
        out.push('*');
    }
    fit_histogram_label(&out, HISTOGRAM_COLUMN_WIDTH.saturating_sub(1).max(1))
}

fn histogram_height(value: u64, max_value: u64, height: usize) -> usize {
    if value == 0 || max_value == 0 || height == 0 {
        return 0;
    }
    let filled = ((value as u128) * (height as u128)).div_ceil(max_value as u128) as usize;
    filled.max(1).min(height)
}

fn heatmap_cell(usage: u64, max_usage: u64) -> String {
    heatmap_cell_for_level(heatmap_level(usage, max_usage))
}

fn heatmap_level(usage: u64, max_usage: u64) -> usize {
    if usage == 0 || max_usage == 0 {
        return 0;
    }
    let ratio = (usage as f64).ln_1p() / (max_usage as f64).ln_1p().max(1.0);
    if ratio <= 0.2 {
        1
    } else if ratio <= 0.4 {
        2
    } else if ratio <= 0.65 {
        3
    } else {
        4
    }
}

fn heatmap_cell_for_level(level: usize) -> String {
    let color = match level {
        0 => 237,
        1 => 22,
        2 => 28,
        3 => 34,
        _ => 46,
    };
    format!("\x1b[38;5;{color}m██\x1b[0m")
}

fn heatmap_legend_line(language: Language) -> String {
    let mut line = format!("{} ", dim(label_less(language)));
    for level in 0..=4 {
        line.push_str(&heatmap_cell_for_level(level));
    }
    line.push(' ');
    line.push_str(&dim(label_more(language)));
    line
}

fn format_number(value: u64) -> String {
    if value < 1_000 {
        return value.to_string();
    }
    if value < 999_500 {
        return format_scaled_number(value, 1_000, "k");
    }
    if value < 999_500_000 {
        return format_scaled_number(value, 1_000_000, "M");
    }
    format_scaled_number(value, 1_000_000_000, "B")
}

fn format_scaled_number(value: u64, divisor: u64, suffix: &str) -> String {
    let scaled = value as f64 / divisor as f64;
    let mut out = if scaled < 100.0 {
        format!("{scaled:.1}")
    } else {
        format!("{scaled:.0}")
    };
    if out.ends_with(".0") {
        out.truncate(out.len() - 2);
    }
    format!("{out}{suffix}")
}

fn format_cost(cost: billing::Cost) -> String {
    let mut out = format_dollars(cost.usd);
    if cost.unpriced_events > 0 {
        out.push('*');
    }
    out
}

fn format_dollars(value: f64) -> String {
    let cents = (value * 100.0).round() as u64;
    let dollars = cents / 100;
    let cents = cents % 100;
    format!("${}.{:02}", format_integer_with_commas(dollars), cents)
}

fn format_integer_with_commas(value: u64) -> String {
    let raw = value.to_string();
    let mut out = String::new();
    for (index, ch) in raw.chars().rev().enumerate() {
        if index > 0 && index % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out.chars().rev().collect()
}

fn home_dir() -> Result<PathBuf> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("HOME is not set; pass --home explicitly"))
}

#[cfg(test)]
mod tests {
    use super::{
        apply_date_filter, describe_counts, fit_histogram_label, format_cost, format_dollars,
        format_number, heatmap_level, histogram_height, localized_warning, max_heatmap_weeks,
        max_histogram_columns, obsidian_dashboard_markdown, obsidian_dashboard_markdown_with_home,
        obsidian_dataview_snapshot_path, source_filter_includes, CivilDate, DataCounts, DateFilter,
        SourceFilter,
    };
    use crate::billing::Cost;
    use crate::config::Language;
    use crate::model::{ReportData, SessionMeta, Source, ToolEvent, Usage, UsageEvent};
    use std::path::Path;

    #[test]
    fn formats_numbers_with_compact_units() {
        assert_eq!(format_number(0), "0");
        assert_eq!(format_number(999), "999");
        assert_eq!(format_number(1_000), "1k");
        assert_eq!(format_number(12_345), "12.3k");
        assert_eq!(format_number(292_631), "293k");
        assert_eq!(format_number(999_500), "1M");
        assert_eq!(format_number(12_697_984), "12.7M");
        assert_eq!(format_number(999_499_999), "999M");
        assert_eq!(format_number(999_500_000), "1B");
        assert_eq!(format_number(1_550_406_823), "1.6B");
        assert_eq!(format_number(2_485_000_000), "2.5B");
    }

    #[test]
    fn formats_costs_in_dollars() {
        assert_eq!(format_dollars(0.0), "$0.00");
        assert_eq!(format_dollars(12.345), "$12.35");
        assert_eq!(format_dollars(1_234.5), "$1,234.50");
    }

    #[test]
    fn marks_partial_costs() {
        assert_eq!(
            format_cost(Cost {
                usd: 1.23,
                unpriced_events: 1,
                unpriced_tokens: 999,
            }),
            "$1.23*"
        );
    }

    #[test]
    fn rejects_inverted_date_filters() {
        let filter = DateFilter::parse(Some("2026-05-10"), Some("2026-05-01"));
        assert!(filter.is_err());
    }

    #[test]
    fn filters_report_data_by_inclusive_dates() {
        let mut data = ReportData {
            sessions: vec![
                session("2026-05-01"),
                session("2026-05-02"),
                session("2026-05-03"),
            ],
            usage_events: vec![
                usage_event("2026-05-01"),
                usage_event("2026-05-02"),
                usage_event("2026-05-03"),
            ],
            tool_events: vec![
                tool_event("2026-05-01"),
                tool_event("2026-05-02"),
                tool_event("2026-05-03"),
            ],
            warnings: Vec::new(),
        };
        let filter = DateFilter {
            since: Some(CivilDate::parse("2026-05-02").unwrap()),
            until: Some(CivilDate::parse("2026-05-02").unwrap()),
        };

        apply_date_filter(&mut data, Some(filter));

        assert_eq!(data.sessions.len(), 1);
        assert_eq!(data.usage_events.len(), 1);
        assert_eq!(data.tool_events.len(), 1);
        assert_eq!(data.sessions[0].date, "2026-05-02");
    }

    #[test]
    fn source_filters_match_expected_doctor_inputs() {
        assert!(source_filter_includes(SourceFilter::All, Source::Claude));
        assert!(source_filter_includes(SourceFilter::All, Source::Codex));
        assert!(source_filter_includes(SourceFilter::All, Source::OpenCode));
        assert!(source_filter_includes(SourceFilter::Claude, Source::Claude));
        assert!(!source_filter_includes(SourceFilter::Claude, Source::Codex));
        assert!(!source_filter_includes(
            SourceFilter::Claude,
            Source::OpenCode
        ));
        assert!(source_filter_includes(SourceFilter::Codex, Source::Codex));
        assert!(!source_filter_includes(SourceFilter::Codex, Source::Claude));
        assert!(source_filter_includes(
            SourceFilter::OpenCode,
            Source::OpenCode
        ));
        assert!(!source_filter_includes(
            SourceFilter::OpenCode,
            Source::Codex
        ));
    }

    #[test]
    fn describes_doctor_counts_compactly() {
        let counts = DataCounts {
            sessions: 2,
            usage_events: 3,
            tool_events: 4,
            total_tokens: 12_345,
        };

        assert_eq!(
            describe_counts(counts, Language::En),
            "2 sessions, 3 usage events, 4 tool calls, 12.3k total tokens"
        );
        assert_eq!(
            describe_counts(counts, Language::Zh),
            "2 会话, 3 用量事件, 4 工具调用, 12.3k 总 token"
        );
    }

    #[test]
    fn builds_obsidian_dashboard_with_configured_snapshot_path() {
        let markdown = obsidian_dashboard_markdown(
            Path::new("data/tokencheck.json"),
            Path::new("2 - Docs/token-check/Token Usage Dashboard.md"),
        );

        assert!(markdown.contains("source: data/tokencheck.json"));
        assert!(markdown.contains(r#"const snapshotPath = "data/tokencheck.json";"#));
        assert!(!markdown.contains("__TOKENCHECK_HOME_PATH__"));
    }

    #[test]
    fn injects_home_path_for_obsidian_display_helpers() {
        let markdown = obsidian_dashboard_markdown_with_home(
            Path::new("/Users/hanlife02/vault/data/tokencheck.json"),
            Path::new("/Users/hanlife02/vault/Token Usage Dashboard.md"),
            Some(Path::new("/Users/hanlife02")),
        );

        assert!(markdown.contains(r#"const configuredHomePath = "/Users/hanlife02";"#));
    }

    #[test]
    fn uses_relative_obsidian_snapshot_paths_directly() {
        assert_eq!(
            obsidian_dataview_snapshot_path(
                Path::new("data/tokencheck.json"),
                Path::new("2 - Docs/token-check/Token Usage Dashboard.md"),
            ),
            "data/tokencheck.json"
        );
    }

    #[test]
    fn maps_usage_to_heatmap_levels() {
        assert_eq!(heatmap_level(0, 100), 0);
        assert_eq!(heatmap_level(1, 100), 1);
        assert_eq!(heatmap_level(10, 100), 3);
        assert_eq!(heatmap_level(50, 100), 4);
        assert_eq!(heatmap_level(100, 100), 4);
    }

    #[test]
    fn scales_histogram_heights() {
        assert_eq!(histogram_height(0, 100, 10), 0);
        assert_eq!(histogram_height(1, 100, 10), 1);
        assert_eq!(histogram_height(50, 100, 10), 5);
        assert_eq!(histogram_height(100, 100, 10), 10);
    }

    #[test]
    fn fits_histogram_labels() {
        assert_eq!(fit_histogram_label("05-13", 7), "05-13");
        assert_eq!(fit_histogram_label("$301.30", 7), "$301.30");
        assert_eq!(fit_histogram_label("$1234.56", 7), "$1234.5");
    }

    #[test]
    fn calculates_chart_columns_from_available_width() {
        assert_eq!(max_histogram_columns(41, 4), 5);
        assert_eq!(max_heatmap_weeks(44), 12);
    }

    #[test]
    fn localizes_known_collector_warnings() {
        assert_eq!(
            localized_warning(
                Language::Zh,
                "Codex data directory not found: /tmp/.codex/sessions"
            ),
            "未找到 Codex 数据目录: /tmp/.codex/sessions"
        );
        assert_eq!(
            localized_warning(Language::En, "Failed to walk Claude Code data: denied"),
            "Failed to walk Claude Code data: denied"
        );
    }

    fn session(date: &str) -> SessionMeta {
        SessionMeta {
            source: Source::Codex,
            session_id: format!("session-{date}"),
            date: date.to_string(),
            project: String::from("/tmp/project"),
            model: String::from("model"),
        }
    }

    fn usage_event(date: &str) -> UsageEvent {
        UsageEvent {
            source: Source::Codex,
            event_id: format!("usage-{date}"),
            session_id: format!("session-{date}"),
            date: date.to_string(),
            project: String::from("/tmp/project"),
            model: String::from("model"),
            usage: Usage {
                total: 1,
                ..Usage::default()
            },
        }
    }

    fn tool_event(date: &str) -> ToolEvent {
        ToolEvent {
            source: Source::Codex,
            event_id: format!("tool-{date}"),
            date: date.to_string(),
            project: String::from("/tmp/project"),
            tool: String::from("shell"),
        }
    }
}
