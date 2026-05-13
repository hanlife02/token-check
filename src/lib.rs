#![forbid(unsafe_code)]

pub mod billing;
pub mod claude_code;
pub mod codex;
pub mod model;

use crate::model::{ReportData, Roots, Source, Usage};
use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand, ValueEnum};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::path::PathBuf;

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
}

#[derive(Clone, Debug, Subcommand)]
enum Command {
    Summary,
    Days,
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
    let roots = Roots {
        home: cli.home.unwrap_or(home_dir()?),
    };
    let data = collect_data(cli.source, &roots)?;
    let command = cli.command.unwrap_or(Command::Summary);
    let shows_cost = command.shows_cost();

    match command {
        Command::Summary => print_summary(&data),
        Command::Days => print_days(&data, cli.limit),
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
    use super::{format_cost, format_dollars, format_number};
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
}
