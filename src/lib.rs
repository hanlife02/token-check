#![forbid(unsafe_code)]

pub mod billing;
pub mod claude_code;
pub mod codex;
pub mod model;
pub mod snapshot;

use crate::model::{ReportData, Roots, Source, Usage};
use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand, ValueEnum};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::path::{Path, PathBuf};

const DEFAULT_DATA_FILE: &str = "data/tokencheck.json";

#[derive(Parser, Debug)]
#[command(name = "tokencheck")]
#[command(about = "Local Claude Code and Codex usage stats")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    #[arg(long, global = true, value_enum, default_value_t = SourceFilter::All)]
    source: SourceFilter,

    #[arg(long, global = true)]
    home: Option<PathBuf>,

    #[arg(long, global = true, default_value_t = 20)]
    limit: usize,

    #[arg(long, global = true)]
    from_json: bool,

    #[arg(long, global = true, default_value = DEFAULT_DATA_FILE)]
    data_file: PathBuf,
}

#[derive(Clone, Debug, Subcommand)]
enum Command {
    Fetch,
    Summary,
    Days,
    Heatmap {
        #[arg(long, default_value_t = 12)]
        months: usize,
    },
    Projects,
    Models,
    Tools,
}

impl Command {
    fn shows_cost(&self) -> bool {
        matches!(
            self,
            Command::Summary | Command::Days | Command::Projects | Command::Models
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum SourceFilter {
    All,
    Claude,
    Codex,
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();
    let command = cli.command.unwrap_or(Command::Summary);
    if matches!(command, Command::Fetch) {
        return run_fetch(cli.source, cli.home, cli.data_file);
    }

    let data = report_data(cli.source, cli.home, &cli.data_file, cli.from_json)?;
    let shows_cost = command.shows_cost();

    match command {
        Command::Fetch => unreachable!("fetch returns before report rendering"),
        Command::Summary => print_summary(&data),
        Command::Days => print_days(&data, cli.limit),
        Command::Heatmap { months } => print_heatmap(&data, months),
        Command::Projects => print_projects(&data, cli.limit),
        Command::Models => print_models(&data, cli.limit),
        Command::Tools => print_tools(&data, cli.limit),
    }

    let mut warnings = data.warnings.clone();
    if shows_cost {
        warnings.extend(billing::unpriced_model_warnings(data.usage_events.iter()));
    }
    if !warnings.is_empty() {
        eprintln!("\nWarnings:");
        for warning in &warnings {
            eprintln!("- {warning}");
        }
    }

    Ok(())
}

fn collect_data(filter: SourceFilter, roots: &Roots) -> Result<ReportData> {
    let mut data = ReportData::default();
    if matches!(filter, SourceFilter::All | SourceFilter::Claude) {
        data.merge(claude_code::collect(&roots.claude_projects())?);
    }
    if matches!(filter, SourceFilter::All | SourceFilter::Codex) {
        data.merge(codex::collect(&roots.codex_sessions())?);
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

fn run_fetch(filter: SourceFilter, home: Option<PathBuf>, data_file: PathBuf) -> Result<()> {
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
        println!("snapshot saved: {}", data_file.display());
    } else {
        println!("snapshot unchanged: {}", data_file.display());
    }
    println!("sessions: {} -> {}", before.sessions, after.sessions);
    println!(
        "usage events: {} -> {} (+{}, upgraded {})",
        before.usage_events,
        after.usage_events,
        summary.usage_events_added,
        summary.usage_events_upgraded
    );
    println!(
        "tool calls: {} -> {} (+{})",
        before.tool_events, after.tool_events, summary.tool_events_added
    );
    println!(
        "total tokens: {} -> {}",
        format_number(before.total_tokens),
        format_number(after.total_tokens)
    );

    if !warnings.is_empty() {
        eprintln!("\nWarnings:");
        for warning in &warnings {
            eprintln!("- {warning}");
        }
    }

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

fn source_matches(source: Source, filter: SourceFilter) -> bool {
    matches!(
        (source, filter),
        (_, SourceFilter::All)
            | (Source::Claude, SourceFilter::Claude)
            | (Source::Codex, SourceFilter::Codex)
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

fn print_summary(data: &ReportData) {
    let mut rows = Vec::new();
    let sources = [Source::Claude, Source::Codex];
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

    let total_stats = sum_usage_stats(data.usage_events.iter());
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
    println!("tokencheck summary");
    println!("sessions scanned: {}", unique_sessions(data, None));
    println!("sessions with usage: {sessions_with_usage}");
    println!("projects seen: {projects_seen}");
    println!("models seen: {models_seen}");
    println!("usage events: {}", data.usage_events.len());
    println!("tool calls: {}", data.tool_events.len());
    println!(
        "total tokens: {}",
        format_number(total_stats.usage.computed_total())
    );
    println!("estimated cost: {}", format_cost(total_stats.cost));
    println!();
    print_table(
        &[
            "source",
            "sessions",
            "usage",
            "tools",
            "input",
            "cached",
            "cache_create",
            "output",
            "reasoning",
            "total",
            "cost",
        ],
        &rows,
    );
}

fn print_days(data: &ReportData, limit: usize) {
    let mut usage_by_date: BTreeMap<String, UsageStats> = BTreeMap::new();
    let mut sessions_by_date: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for event in &data.usage_events {
        usage_by_date
            .entry(event.date.clone())
            .or_default()
            .add(event);
    }
    for session in &data.sessions {
        sessions_by_date
            .entry(session.date.clone())
            .or_default()
            .insert(format!("{}:{}", session.source.label(), session.session_id));
    }

    let rows = usage_by_date
        .into_iter()
        .rev()
        .take(limit)
        .map(|(date, stats)| {
            vec![
                date.clone(),
                sessions_by_date
                    .get(&date)
                    .map(BTreeSet::len)
                    .unwrap_or(0)
                    .to_string(),
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
            "date",
            "sessions",
            "input",
            "cached",
            "cache_create",
            "output",
            "reasoning",
            "total",
            "cost",
        ],
        &rows,
    );
}

fn print_heatmap(data: &ReportData, months: usize) {
    let mut usage_by_date: BTreeMap<CivilDate, UsageStats> = BTreeMap::new();
    for event in &data.usage_events {
        let Some(date) = CivilDate::parse(&event.date) else {
            continue;
        };
        usage_by_date.entry(date).or_default().add(event);
    }

    let Some(latest_date) = usage_by_date.keys().next_back().copied() else {
        println!("No usage data.");
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
    let selected_max = usage_by_date
        .iter()
        .filter(|(date, _)| **date >= start_date && **date <= end_date)
        .map(|(_, stats)| stats.usage.computed_total())
        .max()
        .unwrap_or(0);
    let first_grid_date = start_date.add_days(-(start_date.weekday_sunday_index() as i64));
    let last_grid_date = end_date.add_days((6 - end_date.weekday_sunday_index()) as i64);
    let week_count =
        ((last_grid_date.days_since_epoch() - first_grid_date.days_since_epoch()) / 7 + 1) as usize;

    println!("usage heatmap (daily total tokens)");
    println!("max day: {}", format_number(selected_max));
    print_github_month_labels(first_grid_date, week_count, start_month, latest_month);
    print_github_heatmap_grid(
        first_grid_date,
        week_count,
        start_date,
        end_date,
        selected_max,
        &usage_by_date,
    );
    print_github_heatmap_legend();
}

fn print_github_month_labels(
    first_grid_date: CivilDate,
    week_count: usize,
    start_month: i32,
    latest_month: i32,
) {
    let mut labels = vec![String::from("   "); week_count];
    for month_index in start_month..=latest_month {
        let (year, month) = CivilDate::from_month_index(month_index);
        let first_of_month = CivilDate {
            year,
            month,
            day: 1,
        };
        let week_index =
            ((first_of_month.days_since_epoch() - first_grid_date.days_since_epoch()) / 7) as usize;
        if week_index < labels.len() {
            labels[week_index] = month_abbr(month).to_string();
        }
    }
    println!("     {}", labels.join(""));
}

fn print_github_heatmap_grid(
    first_grid_date: CivilDate,
    week_count: usize,
    start_date: CivilDate,
    end_date: CivilDate,
    max_usage: u64,
    usage_by_date: &BTreeMap<CivilDate, UsageStats>,
) {
    for weekday in 0..7 {
        print!("{:>3}  ", weekday_label(weekday));
        for week in 0..week_count {
            let date = first_grid_date.add_days((week * 7 + weekday) as i64);
            if date < start_date || date > end_date {
                print!("   ");
                continue;
            }
            let usage = usage_by_date
                .get(&date)
                .map(|stats| stats.usage.computed_total())
                .unwrap_or(0);
            print!("{} ", heatmap_cell(usage, max_usage));
        }
        println!();
    }
}

fn print_github_heatmap_legend() {
    print!("     less ");
    for level in [0, 5, 10, 15] {
        print!("{} ", heatmap_cell_for_level(level));
    }
    println!("more");
}

fn print_projects(data: &ReportData, limit: usize) {
    let rows = aggregate_usage(
        data,
        |event| (event.source.label().to_string(), event.project.clone()),
        limit,
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
            "source",
            "project",
            "input",
            "cached",
            "cache_create",
            "output",
            "reasoning",
            "total",
            "cost",
        ],
        &rows,
    );
}

fn print_models(data: &ReportData, limit: usize) {
    let rows = aggregate_usage(
        data,
        |event| (event.source.label().to_string(), event.model.clone()),
        limit,
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
            "source",
            "model",
            "input",
            "cached",
            "cache_create",
            "output",
            "reasoning",
            "total",
            "cost",
        ],
        &rows,
    );
}

fn print_tools(data: &ReportData, limit: usize) {
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

    print_table(&["source", "tool", "calls", "days", "projects"], &rows);
}

fn aggregate_usage<K, F>(data: &ReportData, key_fn: F, limit: usize) -> Vec<(K, UsageStats)>
where
    K: Ord + Clone,
    F: Fn(&crate::model::UsageEvent) -> K,
{
    let mut map: BTreeMap<K, UsageStats> = BTreeMap::new();
    for event in &data.usage_events {
        map.entry(key_fn(event)).or_default().add(event);
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

impl UsageStats {
    fn add(&mut self, event: &crate::model::UsageEvent) {
        self.usage.add_assign(&event.usage);
        self.cost.add_assign(billing::event_cost(event));
    }
}

fn sum_usage_stats<'a>(events: impl Iterator<Item = &'a crate::model::UsageEvent>) -> UsageStats {
    let mut stats = UsageStats::default();
    for event in events {
        stats.add(event);
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

fn print_table(headers: &[&str], rows: &[Vec<String>]) {
    let mut widths = headers
        .iter()
        .map(|header| header.len())
        .collect::<Vec<_>>();
    for row in rows {
        for (index, cell) in row.iter().enumerate() {
            widths[index] = widths[index].max(display_width(cell));
        }
    }

    print_row(
        headers.iter().map(|cell| cell.to_string()).collect(),
        &widths,
    );
    let separator = widths
        .iter()
        .map(|width| "-".repeat(*width))
        .collect::<Vec<_>>();
    print_row(separator, &widths);
    for row in rows {
        print_row(row.clone(), &widths);
    }
}

fn print_row(row: Vec<String>, widths: &[usize]) {
    for (index, cell) in row.iter().enumerate() {
        if index > 0 {
            print!("  ");
        }
        let padding = widths[index].saturating_sub(display_width(cell));
        print!("{cell}{}", " ".repeat(padding));
    }
    println!();
}

fn display_width(value: &str) -> usize {
    value.chars().count()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct CivilDate {
    year: i32,
    month: u8,
    day: u8,
}

impl CivilDate {
    fn parse(value: &str) -> Option<Self> {
        let year = value.get(0..4)?.parse::<i32>().ok()?;
        let month = value.get(5..7)?.parse::<u8>().ok()?;
        let day = value.get(8..10)?.parse::<u8>().ok()?;
        if value.as_bytes().get(4) != Some(&b'-') || value.as_bytes().get(7) != Some(&b'-') {
            return None;
        }
        if !(1..=12).contains(&month) || !(1..=days_in_month(year, month)).contains(&day) {
            return None;
        }
        Some(Self { year, month, day })
    }

    fn month_index(self) -> i32 {
        self.year * 12 + i32::from(self.month) - 1
    }

    fn from_month_index(index: i32) -> (i32, u8) {
        let year = index.div_euclid(12);
        let month = index.rem_euclid(12) + 1;
        (year, month as u8)
    }

    fn days_since_epoch(self) -> i64 {
        days_from_civil(self.year, self.month, self.day)
    }

    fn add_days(self, days: i64) -> Self {
        civil_from_days(self.days_since_epoch() + days)
    }

    fn weekday_sunday_index(self) -> usize {
        let days = days_from_civil(self.year, self.month, self.day);
        (days + 4).rem_euclid(7) as usize
    }
}

fn heatmap_cell(usage: u64, max_usage: u64) -> String {
    heatmap_cell_for_level(heatmap_level(usage, max_usage))
}

fn heatmap_level(usage: u64, max_usage: u64) -> usize {
    if usage == 0 || max_usage == 0 {
        return 0;
    }
    let index = (((usage as u128) * 15 - 1) / (max_usage as u128)) as usize + 1;
    index.min(15)
}

fn heatmap_cell_for_level(level: usize) -> String {
    let color = match level {
        0 => 237,
        1 => 22,
        2 => 28,
        3 => 34,
        4 => 40,
        5 => 46,
        6 => 82,
        7 => 118,
        8 => 154,
        9 => 190,
        10 => 226,
        11 => 220,
        12 => 214,
        13 => 208,
        14 => 202,
        _ => 196,
    };
    format!("\x1b[48;5;{color}m  \x1b[0m")
}

fn month_abbr(month: u8) -> &'static str {
    match month {
        1 => "Jan",
        2 => "Feb",
        3 => "Mar",
        4 => "Apr",
        5 => "May",
        6 => "Jun",
        7 => "Jul",
        8 => "Aug",
        9 => "Sep",
        10 => "Oct",
        11 => "Nov",
        12 => "Dec",
        _ => "   ",
    }
}

fn weekday_label(weekday: usize) -> &'static str {
    match weekday {
        0 => "Sun",
        1 => "Mon",
        2 => "Tue",
        3 => "Wed",
        4 => "Thu",
        5 => "Fri",
        6 => "Sat",
        _ => "",
    }
}

fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_from_civil(year: i32, month: u8, day: u8) -> i64 {
    let year = i64::from(year) - if month <= 2 { 1 } else { 0 };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month = i64::from(month);
    let day = i64::from(day);
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn civil_from_days(days: i64) -> CivilDate {
    let days = days + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    let year = year + if month <= 2 { 1 } else { 0 };

    CivilDate {
        year: year as i32,
        month: month as u8,
        day: day as u8,
    }
}

fn format_number(value: u64) -> String {
    if value < 1_000 {
        return value.to_string();
    }
    if value < 999_500 {
        return format_scaled_number(value, 1_000, "k");
    }
    format_scaled_number(value, 1_000_000, "M")
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
    use super::{format_cost, format_dollars, format_number, heatmap_level, CivilDate};
    use crate::billing::Cost;

    #[test]
    fn formats_numbers_with_compact_units() {
        assert_eq!(format_number(0), "0");
        assert_eq!(format_number(999), "999");
        assert_eq!(format_number(1_000), "1k");
        assert_eq!(format_number(12_345), "12.3k");
        assert_eq!(format_number(292_631), "293k");
        assert_eq!(format_number(999_500), "1M");
        assert_eq!(format_number(12_697_984), "12.7M");
        assert_eq!(format_number(1_550_406_823), "1550M");
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
    fn parses_valid_calendar_dates() {
        assert_eq!(
            CivilDate::parse("2026-05-13"),
            Some(CivilDate {
                year: 2026,
                month: 5,
                day: 13
            })
        );
        assert_eq!(CivilDate::parse("2026-02-29"), None);
        assert_eq!(
            CivilDate::parse("2024-02-29"),
            Some(CivilDate {
                year: 2024,
                month: 2,
                day: 29
            })
        );
    }

    #[test]
    fn maps_weekday_with_sunday_origin() {
        assert_eq!(
            CivilDate::parse("2026-05-10")
                .unwrap()
                .weekday_sunday_index(),
            0
        );
        assert_eq!(
            CivilDate::parse("2026-05-13")
                .unwrap()
                .weekday_sunday_index(),
            3
        );
        assert_eq!(
            CivilDate::parse("2026-05-16")
                .unwrap()
                .weekday_sunday_index(),
            6
        );
    }

    #[test]
    fn adds_days_across_month_boundaries() {
        assert_eq!(
            CivilDate::parse("2026-03-01").unwrap().add_days(-1),
            CivilDate {
                year: 2026,
                month: 2,
                day: 28
            }
        );
        assert_eq!(
            CivilDate::parse("2024-02-28").unwrap().add_days(1),
            CivilDate {
                year: 2024,
                month: 2,
                day: 29
            }
        );
    }

    #[test]
    fn maps_usage_to_heatmap_levels() {
        assert_eq!(heatmap_level(0, 100), 0);
        assert_eq!(heatmap_level(1, 100), 1);
        assert_eq!(heatmap_level(50, 100), 8);
        assert_eq!(heatmap_level(100, 100), 15);
    }
}
