---
name: tokencheck
description: "Use when the user asks how to install, configure, run, troubleshoot, or integrate the tokencheck/tkc CLI for local Claude Code, Codex, OpenCode, and Pi token usage reports, snapshots, pricing, terminal charts, or Obsidian dashboards."
---

# Tokencheck

Use this skill to help users operate the `tokencheck` Rust CLI. The crate is published as `ethan-tkc`; installed binaries are `tkc` and `tokencheck`.

## First Checks

1. Prefer `tkc --version` and `tkc --help` to confirm the installed CLI.
2. Use `tkc doctor` when the user has missing data, wrong paths, unreadable snapshots, pricing-file issues, or Obsidian integration problems.
3. Use `tkc config show` before changing persistent settings.
4. Distinguish report commands from write commands:
   - `tkc fetch` writes or updates the JSON snapshot.
   - `summary`, `days`, `heatmap`, `projects`, `models`, and `tools` read and render reports.
   - `tkc obsidian` writes or updates the Obsidian dashboard Markdown file.

## Installation

Install or update from crates.io:

```bash
cargo install ethan-tkc
cargo install ethan-tkc --force
```

Install only this Codex Skill without cloning the source repository:

```bash
python3 ~/.codex/skills/.system/skill-installer/scripts/install-skill-from-github.py \
  --repo hanlife02/token-check \
  --path skills/tokencheck
```

If old local binaries conflict:

```bash
cargo uninstall tokencheck
cargo install ethan-tkc
```

From a source checkout:

```bash
cargo run --bin tkc -- summary
cargo install --path .
```

## Core Usage

Fast overview:

```bash
tkc
tkc summary
```

Persist scan results:

```bash
tkc fetch
```

Read only from the saved snapshot:

```bash
tkc summary --from-json
tkc days --from-json --limit 30
```

Source filters:

```bash
tkc summary --source claude
tkc models --source codex
tkc days --source opencode
tkc tools --source pi
```

Date filters are inclusive and affect report commands only:

```bash
tkc summary --since 30d
tkc days --since 2026-05-01 --until today
tkc models --from-json --since 2026-01-01 --until 2026-03-31
```

Terminal visualizations:

```bash
tkc days --chant --limit 30
tkc heatmap --months 12
```

## Configuration

Interactive configuration:

```bash
tkc config
```

Show or reset config:

```bash
tkc config show
tkc config reset
```

Blank input keeps and saves the current value. `none`, `null`, or `-` clears optional path fields.

Important config fields:

- `source`: default data source, one of `all`, `claude`, `codex`, `opencode`, or `pi`.
- `data_file`: default JSON snapshot path used by `fetch` and `--from-json`.
- `pricing_file`: optional custom pricing JSON.
- `obsidian_snapshot_file`: JSON snapshot path for Obsidian integration.
- `obsidian_dashboard_file`: Markdown dashboard path for Obsidian integration.
- `limit`: default row/chart count.
- `heatmap_months`: default `heatmap` month count.
- `language`: `en` or `zh`.

## Custom Pricing

Use `--pricing-file` or configure `pricing_file` when unknown proxy models need prices or built-in prices need overrides.

Pricing file format, dollars per 1M tokens:

```json
{
  "models": {
    "my-proxy-model": {
      "input": 2.0,
      "cached_input": 0.5,
      "cache_creation_5m": 3.0,
      "cache_creation_1h": 4.0,
      "output": 8.0
    }
  }
}
```

Rules:

- `input` and `output` are required.
- `cached_input` defaults to `input`.
- `cache_creation_5m` and `cache_creation_1h` default to `0`.
- Model names are normalized like built-in prices: case-insensitive, `_` becomes `-`, and `provider/model` matches by the last segment.
- Custom prices override same-name built-in prices.

## Obsidian Workflow

Configure both Obsidian paths via `tkc config`:

```text
Obsidian snapshot file: /path/to/Vault/data/tokencheck.json
Obsidian dashboard file: /path/to/Vault/2 - Docs/token-check/Token Usage Dashboard.md
```

Generate or update the dashboard:

```bash
tkc obsidian
```

Refresh the snapshot first, then update the dashboard:

```bash
tkc obsidian --fetch
```

The generated dashboard is one Markdown file with five DataviewJS sections: Summary, Recent Days, Top Models, Top Projects, and Top Tools. Summary uses one primary total-token metric plus a compact supporting grid. Recent Days retains the 365-day usage heatmap. The three Top sections render pie charts, show categories with at least 5% share separately, combine smaller categories into `Other`, and reveal the slice label, value, and percentage on hover. The dashboard reads the snapshot with a vault-relative path when possible, usually `data/tokencheck.json`, adapts to Obsidian light and dark themes, and allows the annual heatmap to scroll horizontally in narrow panes.

`tkc obsidian` overwrites the configured dashboard file. It does not delete adjacent section notes created by older versions because users may have edited them.

Obsidian requirements:

- Install and enable the Dataview community plugin.
- Enable Dataview JavaScript queries.
- Open the configured dashboard Markdown file.

## Troubleshooting

Start with:

```bash
tkc doctor
```

Common fixes:

- Missing Claude data: confirm `$HOME/.claude/projects` exists or pass `--home`.
- Missing Codex data: confirm `$HOME/.codex/sessions` exists or pass `--home`.
- Missing OpenCode data: confirm `$HOME/.local/share/opencode` exists or pass `--home`.
- Missing Pi data: confirm `$HOME/.pi/agent/sessions` exists or pass `--home`.
- No saved history: run `tkc fetch`.
- Obsidian dashboard missing data: run `tkc obsidian --fetch`, then reopen or refresh the note.
- JSON file is hidden in Obsidian: enable `Files and links -> Detect all file extensions`, or use the Dashboard Markdown instead of opening JSON directly.
- Pricing warnings: add a custom `pricing_file` or verify model names in `tkc models`.

## Development And Release Checks

When modifying the project, run:

```bash
cargo fmt --all -- --check
cargo test
cargo clippy --all-targets -- -D warnings
cargo publish --dry-run --allow-dirty
```

For releases, bump `Cargo.toml`, ensure `Cargo.lock` follows, commit, and push to `main` to trigger the Cargo publish workflow.
