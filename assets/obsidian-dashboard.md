---
type: dashboard
source: __TOKENCHECK_SNAPSHOT_PATH__
tags:
  - token-check
  - hanlife02
---

# Token Usage Dashboard

## Summary

```dataviewjs
const snapshotPath = "__TOKENCHECK_SNAPSHOT_PATH__";
const configuredHomePath = "__TOKENCHECK_HOME_PATH__";
const number = new Intl.NumberFormat("en-US");

function inferHomePath() {
  const basePath = app?.vault?.adapter?.basePath;
  if (typeof basePath !== "string") return "";
  const normalized = basePath.replace(/\\/g, "/");
  return normalized.match(/^\/Users\/[^/]+/)?.[0] ?? normalized.match(/^[A-Za-z]:\/Users\/[^/]+/)?.[0] ?? "";
}

const homePath = (configuredHomePath || inferHomePath()).replace(/\\/g, "/").replace(/\/+$/, "");

function tokenTotal(usage = {}) {
  return usage.total || (
    (usage.input ?? 0) +
    (usage.cached_input ?? 0) +
    (usage.cache_creation_input ?? 0) +
    (usage.output ?? 0) +
    (usage.reasoning_output ?? 0)
  );
}

function compact(value) {
  if (value >= 1_000_000_000) return `${(value / 1_000_000_000).toFixed(2)}B`;
  if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(1)}M`;
  if (value >= 1_000) return `${(value / 1_000).toFixed(1)}k`;
  return number.format(value);
}

function displayPath(value) {
  const text = String(value ?? "");
  if (!text) return text;
  const normalized = text.replace(/\\/g, "/");
  if (homePath && (normalized === homePath || normalized.startsWith(`${homePath}/`))) {
    return `~${normalized.slice(homePath.length)}`;
  }
  return normalized;
}

function modelName(model = "") {
  const normalized = String(model).trim();
  if (!normalized || normalized === "unknown") return normalized;
  const parts = normalized.split("/").map((part) => part.trim()).filter(Boolean);
  return parts[parts.length - 1] ?? normalized;
}

function modelKey(item = {}) {
  const source = String(item.source ?? "").trim();
  const model = modelName(item.model);
  if (!source || !model || model.includes("unknown")) return "";
  return `${source}/${model}`;
}

try {
  const raw = await app.vault.adapter.read(snapshotPath);
  const data = JSON.parse(raw);
  const sessions = data.sessions ?? [];
  const usageEvents = data.usage_events ?? [];
  const toolEvents = data.tool_events ?? [];
  const totalTokens = usageEvents.reduce((sum, event) => sum + tokenTotal(event.usage), 0);
  const projectCount = new Set(
    [...sessions, ...usageEvents].map((item) => item.project).filter((value) => value && value !== "unknown")
  ).size;
  const modelCount = new Set(
    [...sessions, ...usageEvents].map(modelKey).filter(Boolean)
  ).size;

  dv.table(
    ["Metric", "Value"],
    [
      ["Sessions", number.format(sessions.length)],
      ["Usage events", number.format(usageEvents.length)],
      ["Tool calls", number.format(toolEvents.length)],
      ["Projects", number.format(projectCount)],
      ["Models", number.format(modelCount)],
      ["Total tokens", compact(totalTokens)],
      ["Snapshot", displayPath(snapshotPath)],
    ]
  );
} catch (error) {
  dv.paragraph(`Could not read ${snapshotPath}: ${error.message}`);
}
```

## Recent Days

```dataviewjs
const snapshotPath = "__TOKENCHECK_SNAPSHOT_PATH__";
const number = new Intl.NumberFormat("en-US");
const compactNumber = new Intl.NumberFormat("en-US", { notation: "compact", maximumFractionDigits: 1 });

function tokenTotal(usage = {}) {
  return usage.total || (
    (usage.input || 0) +
    (usage.cached_input || 0) +
    (usage.cache_creation_input || 0) +
    (usage.output || 0) +
    (usage.reasoning_output || 0)
  );
}

function compact(value) {
  return compactNumber.format(value || 0);
}

function appendElement(parent, tag, text = "", options = {}) {
  const element = document.createElement(tag);
  if (text) element.textContent = text;
  for (const [name, value] of Object.entries(options.attrs ?? {})) element.setAttribute(name, value);
  Object.assign(element.style, options.style ?? {});
  parent.appendChild(element);
  return element;
}

function localDateString(date) {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

function addDays(date, amount) {
  const next = new Date(date);
  next.setDate(next.getDate() + amount);
  return next;
}

function percentile(sortedValues, ratio) {
  if (sortedValues.length === 0) return 0;
  return sortedValues[Math.min(sortedValues.length - 1, Math.floor((sortedValues.length - 1) * ratio))];
}

try {
  const raw = await app.vault.adapter.read(snapshotPath);
  const data = JSON.parse(raw);
  const dailyTokens = new Map();
  const dailySessions = new Map();

  for (const event of data.usage_events ?? []) {
    if (!event.date || event.date === "unknown") continue;
    dailyTokens.set(event.date, (dailyTokens.get(event.date) ?? 0) + tokenTotal(event.usage));
  }

  for (const session of data.sessions ?? []) {
    if (!session.date || session.date === "unknown") continue;
    const ids = dailySessions.get(session.date) ?? new Set();
    ids.add(`${session.source}:${session.session_id}`);
    dailySessions.set(session.date, ids);
  }

  if (dailyTokens.size === 0) {
    dv.paragraph("No daily usage found.");
  } else {
    const today = new Date();
    today.setHours(0, 0, 0, 0);
    const rangeStart = addDays(today, -364);
    const gridStart = addDays(rangeStart, -rangeStart.getDay());
    const gridEnd = addDays(today, 6 - today.getDay());
    const cells = [];

    for (let date = new Date(gridStart); date <= gridEnd; date = addDays(date, 1)) {
      const key = localDateString(date);
      const inRange = date >= rangeStart && date <= today;
      cells.push({
        date: new Date(date),
        key,
        inRange,
        tokens: inRange ? (dailyTokens.get(key) ?? 0) : 0,
        sessions: inRange ? (dailySessions.get(key)?.size ?? 0) : 0,
      });
    }

    const positiveValues = cells
      .filter((cell) => cell.inRange && cell.tokens > 0)
      .map((cell) => cell.tokens)
      .sort((a, b) => a - b);
    const thresholds = [
      percentile(positiveValues, 0.25),
      percentile(positiveValues, 0.5),
      percentile(positiveValues, 0.75),
    ];
    const levelFor = (tokens) => {
      if (tokens <= 0) return 0;
      if (tokens <= thresholds[0]) return 1;
      if (tokens <= thresholds[1]) return 2;
      if (tokens <= thresholds[2]) return 3;
      return 4;
    };

    const dark = document.body.classList.contains("theme-dark");
    const colors = dark
      ? ["#161b22", "#0e4429", "#006d32", "#26a641", "#39d353"]
      : ["#ebedf0", "#9be9a8", "#40c463", "#30a14e", "#216e39"];
    const mutedCell = dark ? "#0d1117" : "#f6f8fa";
    const borderColor = dark ? "#30363d" : "rgba(27, 31, 35, 0.06)";
    const cellSize = 11;
    const gap = 3;
    const weekWidth = cellSize + gap;
    const weekCount = Math.ceil(cells.length / 7);
    const heatmapWidth = weekCount * weekWidth - gap;
    const activeDays = cells.filter((cell) => cell.inRange && cell.tokens > 0).length;
    const rangeTokens = cells.reduce((sum, cell) => sum + cell.tokens, 0);

    const section = appendElement(dv.container, "div", "", {
      style: {
        width: "100%",
        maxWidth: "100%",
        minWidth: "0",
        overflow: "hidden",
        boxSizing: "border-box",
        margin: "0.5rem 0 1.5rem",
        padding: "0.9rem 1rem 0.8rem",
        border: `1px solid ${borderColor}`,
        borderRadius: "8px",
        background: "var(--background-primary)",
      },
    });

    appendElement(
      section,
      "div",
      `${rangeStart.toLocaleDateString(undefined, { month: "short", day: "numeric", year: "numeric" })} – ${today.toLocaleDateString(undefined, { month: "short", day: "numeric", year: "numeric" })} · ${number.format(activeDays)} active days · ${compact(rangeTokens)} tokens`,
      {
        attrs: { "aria-label": "Heatmap date range and totals" },
        style: {
          marginBottom: "0.75rem",
          color: "var(--text-muted)",
          fontSize: "0.85em",
          lineHeight: "1.4",
          overflowWrap: "anywhere",
        },
      }
    );

    const scroller = appendElement(section, "div", "", {
      style: {
        width: "100%",
        maxWidth: "100%",
        minWidth: "0",
        overflowX: "auto",
        overflowY: "hidden",
        paddingBottom: "0.35rem",
      },
    });
    const chart = appendElement(scroller, "div", "", {
      attrs: { role: "img", "aria-label": "Daily token usage heatmap for the last year" },
      style: {
        display: "grid",
        gridTemplateColumns: `30px ${heatmapWidth}px`,
        gridTemplateRows: `18px ${7 * cellSize + 6 * gap}px`,
        columnGap: "7px",
        width: `${30 + 7 + heatmapWidth}px`,
        minWidth: "max-content",
      },
    });

    appendElement(chart, "div");
    const months = appendElement(chart, "div", "", {
      style: { position: "relative", height: "18px", fontSize: "10px", color: "var(--text-muted)" },
    });
    const seenMonths = new Set();
    for (const cell of cells) {
      if (!cell.inRange || cell.date.getDate() !== 1) continue;
      const monthKey = `${cell.date.getFullYear()}-${cell.date.getMonth()}`;
      if (seenMonths.has(monthKey)) continue;
      seenMonths.add(monthKey);
      const dayOffset = Math.round((cell.date - gridStart) / 86400000);
      const weekIndex = Math.floor(dayOffset / 7);
      appendElement(months, "span", cell.date.toLocaleDateString(undefined, { month: "short" }), {
        style: { position: "absolute", left: `${weekIndex * weekWidth}px`, whiteSpace: "nowrap" },
      });
    }

    const weekdays = appendElement(chart, "div", "", {
      style: {
        display: "grid",
        gridTemplateRows: `repeat(7, ${cellSize}px)`,
        rowGap: `${gap}px`,
        color: "var(--text-muted)",
        fontSize: "9px",
        lineHeight: `${cellSize}px`,
      },
    });
    ["", "Mon", "", "Wed", "", "Fri", ""].forEach((label) => appendElement(weekdays, "span", label));

    const grid = appendElement(chart, "div", "", {
      style: {
        display: "grid",
        gridAutoFlow: "column",
        gridTemplateRows: `repeat(7, ${cellSize}px)`,
        gridAutoColumns: `${cellSize}px`,
        gap: `${gap}px`,
      },
    });

    const footer = appendElement(section, "div", "", {
      style: {
        display: "flex",
        flexWrap: "wrap",
        justifyContent: "space-between",
        alignItems: "center",
        gap: "0.5rem 1rem",
        marginTop: "0.65rem",
      },
    });
    const detailsLine = appendElement(footer, "div", "", {
      attrs: { "aria-live": "polite" },
      style: {
        minHeight: "1.4em",
        color: "var(--text-muted)",
        fontSize: "0.78em",
        fontVariantNumeric: "tabular-nums",
        lineHeight: "1.4",
      },
    });
    const dateLabelFor = (cell) => cell.date.toLocaleDateString(undefined, {
      weekday: "short",
      month: "short",
      day: "numeric",
      year: "numeric",
    });
    const detailsFor = (cell) => {
      const dateLabel = dateLabelFor(cell);
      if (!cell.inRange) return `${dateLabel} · Outside displayed range`;
      const sessionLabel = cell.sessions === 1 ? "session" : "sessions";
      return `${dateLabel} · ${compact(cell.tokens)} tokens · ${number.format(cell.sessions)} ${sessionLabel}`;
    };
    const defaultCell = cells.find((cell) => cell.key === localDateString(today)) ?? cells[cells.length - 1];
    const showDetails = (cell) => {
      detailsLine.textContent = detailsFor(cell);
    };
    showDetails(defaultCell);

    for (const cell of cells) {
      const level = levelFor(cell.tokens);
      const details = detailsFor(cell);
      const cellElement = appendElement(grid, "span", "", {
        attrs: { "aria-label": details },
        style: {
          width: `${cellSize}px`,
          height: `${cellSize}px`,
          boxSizing: "border-box",
          borderRadius: "2px",
          border: `1px solid ${borderColor}`,
          background: cell.inRange ? colors[level] : mutedCell,
          cursor: "default",
        },
      });
      cellElement.addEventListener("mouseenter", () => showDetails(cell));
    }
    grid.addEventListener("mouseleave", () => showDetails(defaultCell));

    const legend = appendElement(footer, "div", "", {
      style: {
        display: "flex",
        alignItems: "center",
        gap: "4px",
        marginLeft: "auto",
        color: "var(--text-muted)",
        fontSize: "10px",
      },
    });
    appendElement(legend, "span", "Less", { style: { marginRight: "2px" } });
    colors.forEach((color) => appendElement(legend, "span", "", {
      style: {
        width: `${cellSize}px`,
        height: `${cellSize}px`,
        borderRadius: "2px",
        border: `1px solid ${borderColor}`,
        background: color,
      },
    }));
    appendElement(legend, "span", "More", { style: { marginLeft: "2px" } });
  }
} catch (error) {
  dv.paragraph(`Could not read ${snapshotPath}: ${error.message}`);
}
```

## Top Models

```dataviewjs
const snapshotPath = "__TOKENCHECK_SNAPSHOT_PATH__";

function tokenTotal(usage = {}) {
  return usage.total || ((usage.input ?? 0) + (usage.cached_input ?? 0) + (usage.cache_creation_input ?? 0) + (usage.output ?? 0) + (usage.reasoning_output ?? 0));
}

function modelName(model = "") {
  const normalized = String(model).trim();
  if (!normalized || normalized === "unknown") return normalized;
  const parts = normalized.split("/").map((part) => part.trim()).filter(Boolean);
  return parts[parts.length - 1] ?? normalized;
}

function modelKey(item = {}) {
  const source = String(item.source ?? "").trim();
  const model = modelName(item.model);
  if (!source || !model || model.includes("unknown")) return "";
  return `${source}/${model}`;
}

function compact(value) {
  return new Intl.NumberFormat("en-US", { notation: "compact", maximumFractionDigits: 1 }).format(value || 0);
}

function appendElement(parent, tag, text = "", options = {}) {
  const element = document.createElement(tag);
  if (text) element.textContent = text;
  for (const [name, value] of Object.entries(options.attrs ?? {})) element.setAttribute(name, value);
  Object.assign(element.style, options.style ?? {});
  parent.appendChild(element);
  return element;
}

function groupBy(items, keyFn, valueFn) {
  const map = new Map();
  for (const item of items) {
    const key = keyFn(item);
    if (!key || key.includes("unknown")) continue;
    map.set(key, (map.get(key) ?? 0) + valueFn(item));
  }
  return [...map.entries()].sort((a, b) => b[1] - a[1]);
}

function percentage(value, total) {
  if (!total || !value) return "0%";
  const share = (value / total) * 100;
  return share < 0.1 ? "<0.1%" : `${share.toFixed(1)}%`;
}

function renderRanking(items, total, unit, accent) {
  if (items.length === 0) {
    dv.paragraph(`No ${unit} data found.`);
    return;
  }

  const maxValue = Math.max(...items.map(([, value]) => value), 0);
  const chart = appendElement(dv.container, "div", "", {
    attrs: { role: "list", "aria-label": `Top ${items.length} ${unit}` },
    style: {
      width: "100%",
      maxWidth: "100%",
      minWidth: "0",
      boxSizing: "border-box",
      margin: "0.5rem 0 1.5rem",
      padding: "0.8rem 1rem 0.65rem",
      border: "1px solid var(--background-modifier-border)",
      borderRadius: "8px",
      background: "var(--background-primary)",
    },
  });

  appendElement(chart, "div", `Top ${items.length} · ${compact(total)} ${unit} total`, {
    style: { marginBottom: "0.35rem", color: "var(--text-muted)", fontSize: "0.8em" },
  });

  items.forEach(([label, value], index) => {
    const share = percentage(value, total);
    const width = maxValue ? Math.max(1.5, (value / maxValue) * 100) : 0;
    const details = `${index + 1}. ${label}: ${compact(value)} ${unit}, ${share} of total`;
    const row = appendElement(chart, "div", "", {
      attrs: { role: "listitem", title: details, "aria-label": details },
      style: {
        display: "grid",
        gridTemplateColumns: "24px minmax(0, 1fr)",
        columnGap: "0.55rem",
        padding: "0.55rem 0",
        borderBottom: index < items.length - 1 ? "1px solid var(--background-modifier-border)" : "none",
      },
    });

    appendElement(row, "span", String(index + 1), {
      style: {
        color: "var(--text-faint)",
        fontSize: "0.78em",
        fontVariantNumeric: "tabular-nums",
        lineHeight: "1.5",
        textAlign: "right",
      },
    });

    const content = appendElement(row, "div", "", { style: { minWidth: "0" } });
    const heading = appendElement(content, "div", "", {
      style: {
        display: "flex",
        justifyContent: "space-between",
        alignItems: "baseline",
        gap: "0.75rem",
        minWidth: "0",
      },
    });
    appendElement(heading, "span", label, {
      attrs: { title: label },
      style: {
        minWidth: "0",
        overflow: "hidden",
        color: "var(--text-normal)",
        fontSize: "0.88em",
        fontWeight: "600",
        textOverflow: "ellipsis",
        whiteSpace: "nowrap",
      },
    });
    appendElement(heading, "span", `${compact(value)} · ${share}`, {
      style: {
        flexShrink: "0",
        color: "var(--text-muted)",
        fontSize: "0.76em",
        fontVariantNumeric: "tabular-nums",
        whiteSpace: "nowrap",
      },
    });

    const track = appendElement(content, "div", "", {
      style: {
        height: "8px",
        marginTop: "0.38rem",
        overflow: "hidden",
        borderRadius: "999px",
        background: "var(--background-modifier-border)",
      },
    });
    appendElement(track, "div", "", {
      style: {
        width: `${width}%`,
        height: "100%",
        borderRadius: "999px",
        background: accent,
      },
    });
  });
}

try {
  const raw = await app.vault.adapter.read(snapshotPath);
  const data = JSON.parse(raw);
  const ranked = groupBy(data.usage_events ?? [], modelKey, (event) => tokenTotal(event.usage));
  const total = ranked.reduce((sum, [, value]) => sum + value, 0);
  const accent = document.body.classList.contains("theme-dark") ? "#3fb950" : "#2da44e";
  renderRanking(ranked.slice(0, 10), total, "tokens", accent);
} catch (error) {
  dv.paragraph(`Could not read ${snapshotPath}: ${error.message}`);
}
```

## Top Projects

```dataviewjs
const snapshotPath = "__TOKENCHECK_SNAPSHOT_PATH__";
const configuredHomePath = "__TOKENCHECK_HOME_PATH__";

function inferHomePath() {
  const basePath = app?.vault?.adapter?.basePath;
  if (typeof basePath !== "string") return "";
  const normalized = basePath.replace(/\\/g, "/");
  return normalized.match(/^\/Users\/[^/]+/)?.[0] ?? normalized.match(/^[A-Za-z]:\/Users\/[^/]+/)?.[0] ?? "";
}

const homePath = (configuredHomePath || inferHomePath()).replace(/\\/g, "/").replace(/\/+$/, "");

function tokenTotal(usage = {}) {
  return usage.total || ((usage.input ?? 0) + (usage.cached_input ?? 0) + (usage.cache_creation_input ?? 0) + (usage.output ?? 0) + (usage.reasoning_output ?? 0));
}

function displayPath(value) {
  const text = String(value ?? "");
  if (!text) return text;
  const normalized = text.replace(/\\/g, "/");
  if (homePath && (normalized === homePath || normalized.startsWith(`${homePath}/`))) return `~${normalized.slice(homePath.length)}`;
  return normalized;
}

function compact(value) {
  return new Intl.NumberFormat("en-US", { notation: "compact", maximumFractionDigits: 1 }).format(value || 0);
}

function appendElement(parent, tag, text = "", options = {}) {
  const element = document.createElement(tag);
  if (text) element.textContent = text;
  for (const [name, value] of Object.entries(options.attrs ?? {})) element.setAttribute(name, value);
  Object.assign(element.style, options.style ?? {});
  parent.appendChild(element);
  return element;
}

function groupBy(items, keyFn, valueFn) {
  const map = new Map();
  for (const item of items) {
    const key = keyFn(item);
    if (!key || key.includes("unknown")) continue;
    map.set(key, (map.get(key) ?? 0) + valueFn(item));
  }
  return [...map.entries()].sort((a, b) => b[1] - a[1]);
}

function percentage(value, total) {
  if (!total || !value) return "0%";
  const share = (value / total) * 100;
  return share < 0.1 ? "<0.1%" : `${share.toFixed(1)}%`;
}

function renderRanking(items, total, unit, accent) {
  if (items.length === 0) {
    dv.paragraph(`No ${unit} data found.`);
    return;
  }

  const maxValue = Math.max(...items.map(([, value]) => value), 0);
  const chart = appendElement(dv.container, "div", "", {
    attrs: { role: "list", "aria-label": `Top ${items.length} ${unit}` },
    style: {
      width: "100%",
      maxWidth: "100%",
      minWidth: "0",
      boxSizing: "border-box",
      margin: "0.5rem 0 1.5rem",
      padding: "0.8rem 1rem 0.65rem",
      border: "1px solid var(--background-modifier-border)",
      borderRadius: "8px",
      background: "var(--background-primary)",
    },
  });

  appendElement(chart, "div", `Top ${items.length} · ${compact(total)} ${unit} total`, {
    style: { marginBottom: "0.35rem", color: "var(--text-muted)", fontSize: "0.8em" },
  });

  items.forEach(([label, value], index) => {
    const share = percentage(value, total);
    const width = maxValue ? Math.max(1.5, (value / maxValue) * 100) : 0;
    const details = `${index + 1}. ${label}: ${compact(value)} ${unit}, ${share} of total`;
    const row = appendElement(chart, "div", "", {
      attrs: { role: "listitem", title: details, "aria-label": details },
      style: {
        display: "grid",
        gridTemplateColumns: "24px minmax(0, 1fr)",
        columnGap: "0.55rem",
        padding: "0.55rem 0",
        borderBottom: index < items.length - 1 ? "1px solid var(--background-modifier-border)" : "none",
      },
    });

    appendElement(row, "span", String(index + 1), {
      style: {
        color: "var(--text-faint)",
        fontSize: "0.78em",
        fontVariantNumeric: "tabular-nums",
        lineHeight: "1.5",
        textAlign: "right",
      },
    });

    const content = appendElement(row, "div", "", { style: { minWidth: "0" } });
    const heading = appendElement(content, "div", "", {
      style: {
        display: "flex",
        justifyContent: "space-between",
        alignItems: "baseline",
        gap: "0.75rem",
        minWidth: "0",
      },
    });
    appendElement(heading, "span", label, {
      attrs: { title: label },
      style: {
        minWidth: "0",
        overflow: "hidden",
        color: "var(--text-normal)",
        fontSize: "0.88em",
        fontWeight: "600",
        textOverflow: "ellipsis",
        whiteSpace: "nowrap",
      },
    });
    appendElement(heading, "span", `${compact(value)} · ${share}`, {
      style: {
        flexShrink: "0",
        color: "var(--text-muted)",
        fontSize: "0.76em",
        fontVariantNumeric: "tabular-nums",
        whiteSpace: "nowrap",
      },
    });

    const track = appendElement(content, "div", "", {
      style: {
        height: "8px",
        marginTop: "0.38rem",
        overflow: "hidden",
        borderRadius: "999px",
        background: "var(--background-modifier-border)",
      },
    });
    appendElement(track, "div", "", {
      style: {
        width: `${width}%`,
        height: "100%",
        borderRadius: "999px",
        background: accent,
      },
    });
  });
}

try {
  const raw = await app.vault.adapter.read(snapshotPath);
  const data = JSON.parse(raw);
  const ranked = groupBy(data.usage_events ?? [], (event) => displayPath(event.project), (event) => tokenTotal(event.usage));
  const total = ranked.reduce((sum, [, value]) => sum + value, 0);
  const accent = document.body.classList.contains("theme-dark") ? "#58a6ff" : "#0969da";
  renderRanking(ranked.slice(0, 10), total, "tokens", accent);
} catch (error) {
  dv.paragraph(`Could not read ${snapshotPath}: ${error.message}`);
}
```

## Top Tools

```dataviewjs
const snapshotPath = "__TOKENCHECK_SNAPSHOT_PATH__";

function compact(value) {
  return new Intl.NumberFormat("en-US", { notation: "compact", maximumFractionDigits: 1 }).format(value || 0);
}

function appendElement(parent, tag, text = "", options = {}) {
  const element = document.createElement(tag);
  if (text) element.textContent = text;
  for (const [name, value] of Object.entries(options.attrs ?? {})) element.setAttribute(name, value);
  Object.assign(element.style, options.style ?? {});
  parent.appendChild(element);
  return element;
}

function groupBy(items, keyFn, valueFn) {
  const map = new Map();
  for (const item of items) {
    const key = keyFn(item);
    if (!key || key.includes("unknown")) continue;
    map.set(key, (map.get(key) ?? 0) + valueFn(item));
  }
  return [...map.entries()].sort((a, b) => b[1] - a[1]);
}

function percentage(value, total) {
  if (!total || !value) return "0%";
  const share = (value / total) * 100;
  return share < 0.1 ? "<0.1%" : `${share.toFixed(1)}%`;
}

function renderRanking(items, total, unit, accent) {
  if (items.length === 0) {
    dv.paragraph(`No ${unit} data found.`);
    return;
  }

  const maxValue = Math.max(...items.map(([, value]) => value), 0);
  const chart = appendElement(dv.container, "div", "", {
    attrs: { role: "list", "aria-label": `Top ${items.length} ${unit}` },
    style: {
      width: "100%",
      maxWidth: "100%",
      minWidth: "0",
      boxSizing: "border-box",
      margin: "0.5rem 0 1.5rem",
      padding: "0.8rem 1rem 0.65rem",
      border: "1px solid var(--background-modifier-border)",
      borderRadius: "8px",
      background: "var(--background-primary)",
    },
  });

  appendElement(chart, "div", `Top ${items.length} · ${compact(total)} ${unit} total`, {
    style: { marginBottom: "0.35rem", color: "var(--text-muted)", fontSize: "0.8em" },
  });

  items.forEach(([label, value], index) => {
    const share = percentage(value, total);
    const width = maxValue ? Math.max(1.5, (value / maxValue) * 100) : 0;
    const details = `${index + 1}. ${label}: ${compact(value)} ${unit}, ${share} of total`;
    const row = appendElement(chart, "div", "", {
      attrs: { role: "listitem", title: details, "aria-label": details },
      style: {
        display: "grid",
        gridTemplateColumns: "24px minmax(0, 1fr)",
        columnGap: "0.55rem",
        padding: "0.55rem 0",
        borderBottom: index < items.length - 1 ? "1px solid var(--background-modifier-border)" : "none",
      },
    });

    appendElement(row, "span", String(index + 1), {
      style: {
        color: "var(--text-faint)",
        fontSize: "0.78em",
        fontVariantNumeric: "tabular-nums",
        lineHeight: "1.5",
        textAlign: "right",
      },
    });

    const content = appendElement(row, "div", "", { style: { minWidth: "0" } });
    const heading = appendElement(content, "div", "", {
      style: {
        display: "flex",
        justifyContent: "space-between",
        alignItems: "baseline",
        gap: "0.75rem",
        minWidth: "0",
      },
    });
    appendElement(heading, "span", label, {
      attrs: { title: label },
      style: {
        minWidth: "0",
        overflow: "hidden",
        color: "var(--text-normal)",
        fontSize: "0.88em",
        fontWeight: "600",
        textOverflow: "ellipsis",
        whiteSpace: "nowrap",
      },
    });
    appendElement(heading, "span", `${compact(value)} · ${share}`, {
      style: {
        flexShrink: "0",
        color: "var(--text-muted)",
        fontSize: "0.76em",
        fontVariantNumeric: "tabular-nums",
        whiteSpace: "nowrap",
      },
    });

    const track = appendElement(content, "div", "", {
      style: {
        height: "8px",
        marginTop: "0.38rem",
        overflow: "hidden",
        borderRadius: "999px",
        background: "var(--background-modifier-border)",
      },
    });
    appendElement(track, "div", "", {
      style: {
        width: `${width}%`,
        height: "100%",
        borderRadius: "999px",
        background: accent,
      },
    });
  });
}

try {
  const raw = await app.vault.adapter.read(snapshotPath);
  const data = JSON.parse(raw);
  const ranked = groupBy(data.tool_events ?? [], (event) => `${event.source}/${event.tool}`, () => 1);
  const total = ranked.reduce((sum, [, value]) => sum + value, 0);
  const accent = document.body.classList.contains("theme-dark") ? "#a371f7" : "#8250df";
  renderRanking(ranked.slice(0, 10), total, "calls", accent);
} catch (error) {
  dv.paragraph(`Could not read ${snapshotPath}: ${error.message}`);
}
```
