#!/usr/bin/env python3
"""Tokscale Terminal Dashboard — 读取导出的 JSON 数据并用 Rich 渲染。"""

import argparse
import json
import os
from datetime import datetime, timedelta
from pathlib import Path

from rich.console import Console
from rich.panel import Panel
from rich.table import Table
from rich.text import Text
from rich.columns import Columns
from rich.style import Style
from rich import box


console = Console(highlight=False, force_terminal=True)


def fmt_tokens(n: int) -> str:
    """格式化 token 数量。"""
    if n >= 1_000_000_000:
        return f"{n / 1_000_000_000:.1f}B"
    if n >= 1_000_000:
        return f"{n / 1_000_000:.1f}M"
    if n >= 1_000:
        return f"{n / 1_000:.1f}K"
    return str(n)


def fmt_cost(c: float) -> str:
    """格式化成本。"""
    if c >= 1000:
        return f"${c:,.0f}"
    if c >= 1:
        return f"${c:,.2f}"
    if c > 0:
        return f"${c:.4f}"
    return "$0.00"


# ── Models 面板 ──────────────────────────────────────────────────────────────

def render_models(data_dir: Path):
    path = data_dir / "models.json"
    if not path.exists():
        console.print(f"[red]找不到 {path}[/red]")
        return
    data = json.loads(path.read_text())

    table = Table(
        title="Models",
        box=box.ROUNDED,
        show_lines=False,
        header_style="bold cyan",
        title_style="bold white",
    )
    table.add_column("Model", style="bold white", min_width=22)
    table.add_column("Provider", style="dim")
    table.add_column("Client", style="green")
    table.add_column("Input", justify="right")
    table.add_column("Output", justify="right")
    table.add_column("Cache Read", justify="right")
    table.add_column("Cache Write", justify="right")
    table.add_column("Msgs", justify="right", style="dim")
    table.add_column("Cost", justify="right", style="bold yellow")

    total_input = total_output = total_cr = total_cw = total_msgs = 0
    total_cost = 0.0

    for e in data.get("entries", []):
        table.add_row(
            e["model"],
            e.get("provider", "—"),
            e.get("client", "—"),
            fmt_tokens(e.get("input", 0)),
            fmt_tokens(e.get("output", 0)),
            fmt_tokens(e.get("cacheRead", 0)),
            fmt_tokens(e.get("cacheWrite", 0)),
            str(e.get("messageCount", 0)),
            fmt_cost(e.get("cost", 0)),
        )
        total_input += e.get("input", 0)
        total_output += e.get("output", 0)
        total_cr += e.get("cacheRead", 0)
        total_cw += e.get("cacheWrite", 0)
        total_msgs += e.get("messageCount", 0)
        total_cost += e.get("cost", 0)

    table.add_section()
    table.add_row(
        "[bold]TOTAL[/bold]", "", "",
        f"[bold]{fmt_tokens(total_input)}[/bold]",
        f"[bold]{fmt_tokens(total_output)}[/bold]",
        f"[bold]{fmt_tokens(total_cr)}[/bold]",
        f"[bold]{fmt_tokens(total_cw)}[/bold]",
        f"[bold]{total_msgs}[/bold]",
        f"[bold yellow]{fmt_cost(total_cost)}[/bold yellow]",
    )

    console.print(table)
    console.print()


# ── Monthly 面板 ─────────────────────────────────────────────────────────────

def render_monthly(data_dir: Path):
    path = data_dir / "monthly.json"
    if not path.exists():
        console.print(f"[red]找不到 {path}[/red]")
        return
    data = json.loads(path.read_text())

    table = Table(
        title="Monthly",
        box=box.ROUNDED,
        header_style="bold cyan",
        title_style="bold white",
    )
    table.add_column("Month", style="bold white")
    table.add_column("Models", style="dim", max_width=40)
    table.add_column("Input", justify="right")
    table.add_column("Output", justify="right")
    table.add_column("Cache Read", justify="right")
    table.add_column("Messages", justify="right")
    table.add_column("Cost", justify="right", style="bold yellow")

    for e in data.get("entries", []):
        models_str = ", ".join(e.get("models", []))
        table.add_row(
            e["month"],
            models_str,
            fmt_tokens(e.get("input", 0)),
            fmt_tokens(e.get("output", 0)),
            fmt_tokens(e.get("cacheRead", 0)),
            str(e.get("messageCount", 0)),
            fmt_cost(e.get("cost", 0)),
        )

    console.print(table)
    console.print()


# ── Stats 面板 ───────────────────────────────────────────────────────────────

def render_stats(data_dir: Path):
    path = data_dir / "graph.json"
    if not path.exists():
        console.print(f"[red]找不到 {path}[/red]")
        return
    data = json.loads(path.read_text())

    summary = data.get("summary", {})
    contributions = data.get("contributions", [])

    # 概览指标
    stats_table = Table(
        title="Stats Overview",
        box=box.ROUNDED,
        header_style="bold cyan",
        title_style="bold white",
    )
    stats_table.add_column("Metric", style="bold")
    stats_table.add_column("Value", justify="right", style="bold yellow")

    stats_table.add_row("Total Tokens", fmt_tokens(summary.get("totalTokens", 0)))
    stats_table.add_row("Total Cost", fmt_cost(summary.get("totalCost", 0)))
    stats_table.add_row("Active Days", str(summary.get("activeDays", 0)))
    stats_table.add_row("Total Days", str(summary.get("totalDays", 0)))
    avg = summary.get("totalCost", 0) / max(summary.get("activeDays", 1), 1)
    stats_table.add_row("Avg Cost / Day", fmt_cost(avg))
    stats_table.add_row("Peak Day Cost", fmt_cost(summary.get("maxCostInSingleDay", 0)))

    clients = summary.get("clients", [])
    stats_table.add_row("Clients", ", ".join(clients))
    models = summary.get("models", [])
    stats_table.add_row("Models", ", ".join(models))

    console.print(stats_table)
    console.print()

    # GitHub 风格贡献热力图
    _render_heatmap(contributions)


def _render_heatmap(contributions: list):
    """用 Rich 渲染 GitHub 风格的周历热力图。"""
    if not contributions:
        console.print("[dim]No contribution data available.[/dim]")
        return

    # 构建 date -> cost 映射
    date_cost: dict[str, float] = {}
    for c in contributions:
        date_cost[c["date"]] = c["totals"]["cost"]

    # 确定日期范围
    all_dates = sorted(date_cost.keys())
    start = datetime.strptime(all_dates[0], "%Y-%m-%d")
    end = datetime.strptime(all_dates[-1], "%Y-%m-%d")

    # 扩展到完整周（从周日开始）
    while start.weekday() != 6:  # 6 = Sunday
        start -= timedelta(days=1)
    # 结束日期 = 数据最后一天所在周的周六
    end_day = datetime.strptime(all_dates[-1], "%Y-%m-%d")
    while end_day.weekday() != 5:  # 5 = Saturday
        end_day += timedelta(days=1)

    # 计算强度等级
    max_cost = max(date_cost.values()) if date_cost else 1

    # 按周组织（每列 = 一周，7 行 = 周日到周六）
    weeks: list[list[tuple[str, float]]] = []
    current_week: list[tuple[str, float]] = []
    d = start
    while d <= end_day:
        ds = d.strftime("%Y-%m-%d")
        cost = date_cost.get(ds, 0)
        current_week.append((ds, cost))
        if len(current_week) == 7:
            weeks.append(current_week)
            current_week = []
        d += timedelta(days=1)
    if current_week:
        # 补齐最后一周
        while len(current_week) < 7:
            current_week.append(("", -1))
        weeks.append(current_week)

    # 月份标签
    month_labels: list[tuple[int, str]] = []
    last_month = ""
    for i, week in enumerate(weeks):
        for ds, _ in week:
            if ds:
                month = datetime.strptime(ds, "%Y-%m-%d").strftime("%b")
                if month != last_month:
                    month_labels.append((i, month))
                    last_month = month
                break

    # 渲染
    console.print("[bold]Contribution Heatmap[/bold]")
    console.print()

    # 月份行
    month_line = Text("        ")
    prev_end = 8
    for idx, label in month_labels:
        target = 8 + idx * 3
        gap = max(0, target - prev_end)
        month_line.append(" " * gap + label, style="dim")
        prev_end = target + len(label)
    console.print(month_line)

    day_names = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"]
    for day_idx in range(7):
        line = Text(f"{day_names[day_idx]:>6}  ")
        for week in weeks:
            if day_idx < len(week):
                ds, cost = week[day_idx]
                if not ds or cost < 0:
                    line.append("  ")
                else:
                    line.append("██", style=_heatmap_style(cost, max_cost))
            else:
                line.append("  ")
        console.print(line)

    # 图例
    console.print()
    legend = Text("        Less ")
    legend.append("██", style=_heatmap_style(0, max_cost))
    legend.append("██", style=_heatmap_style(max_cost * 0.1, max_cost))
    legend.append("██", style=_heatmap_style(max_cost * 0.35, max_cost))
    legend.append("██", style=_heatmap_style(max_cost * 0.65, max_cost))
    legend.append("██", style=_heatmap_style(max_cost, max_cost))
    legend.append(" More")
    console.print(legend)
    console.print()


def _heatmap_style(cost: float, max_cost: float) -> Style:
    """返回 Rich Style 对象。使用 log scale 让低值天也有层次感。"""
    import math
    if cost <= 0:
        return Style(bgcolor="color(237)")   # 灰色，无活动
    log_val = math.log1p(cost)
    log_max = math.log1p(max_cost)
    ratio = log_val / max(log_max, 1)
    if ratio <= 0.2:
        return Style(bgcolor="color(22)")    # 深绿
    if ratio <= 0.4:
        return Style(bgcolor="color(28)")    # 中绿
    if ratio <= 0.65:
        return Style(bgcolor="color(34)")    # 亮绿
    return Style(bgcolor="color(46)")        # 最亮绿


# ── 主入口 ───────────────────────────────────────────────────────────────────

def main():
    parser = argparse.ArgumentParser(description="Tokscale Terminal Dashboard")
    parser.add_argument(
        "--dir",
        type=str,
        default=os.path.expanduser("~/code/token-check/data/latest"),
        help="数据目录路径（默认 latest）",
    )
    args = parser.parse_args()

    data_dir = Path(args.dir)
    if not data_dir.exists():
        console.print(f"[red]数据目录不存在: {data_dir}[/red]")
        console.print("[dim]请先运行 ~/code/token-check/scripts/daily-export.sh 导出数据[/dim]")
        raise SystemExit(1)

    console.print()
    console.print(Panel.fit(
        "[bold white]Tokscale Dashboard[/bold white]",
        subtitle=f"[dim]{data_dir}[/dim]",
        border_style="cyan",
    ))
    console.print()

    render_models(data_dir)
    render_monthly(data_dir)
    render_stats(data_dir)


if __name__ == "__main__":
    main()
