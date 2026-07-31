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

function appendElement(parent, tag, text = "", options = {}) {
  const element = document.createElement(tag);
  if (text) element.textContent = text;
  for (const [name, value] of Object.entries(options.attrs ?? {})) element.setAttribute(name, value);
  Object.assign(element.style, options.style ?? {});
  parent.appendChild(element);
  return element;
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

  const summary = appendElement(dv.container, "div", "", {
    attrs: { "aria-label": "Token usage summary" },
    style: { margin: "0.5rem 0 1.5rem" },
  });
  const primary = appendElement(summary, "div", "", {
    style: {
      padding: "0.75rem 0 1rem",
      borderBottom: "1px solid var(--background-modifier-border)",
    },
  });
  appendElement(primary, "div", "Total tokens", {
    style: { color: "var(--text-muted)", fontSize: "0.8rem", fontWeight: "600" },
  });
  appendElement(primary, "div", compact(totalTokens), {
    attrs: { title: number.format(totalTokens) },
    style: {
      marginTop: "0.25rem",
      color: "var(--text-normal)",
      fontSize: "2rem",
      fontWeight: "700",
      fontVariantNumeric: "tabular-nums",
      lineHeight: "1.1",
    },
  });

  const summaryGrid = appendElement(summary, "div", "", {
    style: {
      display: "grid",
      gridTemplateColumns: "repeat(auto-fit, minmax(128px, 1fr))",
      gap: "0.5rem",
      padding: "1rem 0 0.75rem",
    },
  });
  [
    ["Sessions", sessions.length],
    ["Usage events", usageEvents.length],
    ["Tool calls", toolEvents.length],
    ["Projects", projectCount],
    ["Models", modelCount],
  ].forEach(([label, value]) => {
    const metric = appendElement(summaryGrid, "div", "", {
      style: {
        minWidth: "0",
        padding: "0.75rem",
        borderRadius: "6px",
        background: "var(--background-secondary)",
      },
    });
    appendElement(metric, "div", label, {
      style: { color: "var(--text-muted)", fontSize: "0.75rem", lineHeight: "1.4" },
    });
    appendElement(metric, "div", number.format(value), {
      style: {
        marginTop: "0.25rem",
        color: "var(--text-normal)",
        fontSize: "1.15rem",
        fontWeight: "650",
        fontVariantNumeric: "tabular-nums",
        lineHeight: "1.2",
      },
    });
  });

  appendElement(summary, "div", `Snapshot · ${displayPath(snapshotPath)}`, {
    attrs: { title: displayPath(snapshotPath) },
    style: {
      overflow: "hidden",
      color: "var(--text-faint)",
      fontSize: "0.75rem",
      lineHeight: "1.4",
      textOverflow: "ellipsis",
      whiteSpace: "nowrap",
    },
  });
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
const minimumShare = 0.05;

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

function groupSmallSlices(items, total) {
  const slices = [];
  let otherValue = 0;
  for (const [label, value] of items) {
    if (total > 0 && value / total >= minimumShare) {
      slices.push([label, value]);
    } else {
      otherValue += value;
    }
  }
  if (otherValue > 0) slices.push(["Other", otherValue]);
  return slices;
}

function showSliceDetails(tooltip, segment, total, unit) {
  if (!segment) {
    tooltip.textContent = "";
    tooltip.style.opacity = "0";
    return;
  }
  tooltip.textContent = `${segment.label} · ${compact(segment.value)} ${unit} · ${percentage(segment.value, total)}`;
  tooltip.style.opacity = "1";
}

function renderPieChart(items, total, unit, palette, otherColor) {
  if (items.length === 0 || total <= 0) {
    dv.paragraph(`No ${unit} data found.`);
    return;
  }

  const slices = groupSmallSlices(items, total);
  let offset = 0;
  const segments = slices.map(([label, value], index) => {
    const start = offset;
    offset += (value / total) * 100;
    return {
      label,
      value,
      start,
      end: offset,
      color: label === "Other" ? otherColor : palette[index % palette.length],
    };
  });
  const details = segments
    .map(({ label, value }) => `${label}: ${compact(value)} ${unit}, ${percentage(value, total)}`)
    .join("; ");
  const chart = appendElement(dv.container, "div", "", {
    attrs: { role: "group", "aria-label": `${unit} distribution` },
    style: {
      display: "flex",
      flexWrap: "wrap",
      alignItems: "center",
      gap: "1.5rem",
      width: "100%",
      maxWidth: "100%",
      minWidth: "0",
      boxSizing: "border-box",
      margin: "0.5rem 0 1.5rem",
      padding: "1rem 0",
      borderTop: "1px solid var(--background-modifier-border)",
      borderBottom: "1px solid var(--background-modifier-border)",
    },
  });

  const plotArea = appendElement(chart, "div", "", {
    style: {
      display: "flex",
      flex: "1 1 220px",
      flexDirection: "column",
      alignItems: "center",
      minWidth: "0",
    },
  });
  const plot = appendElement(plotArea, "div", "", {
    attrs: { role: "img", "aria-label": details },
    style: {
      width: "220px",
      maxWidth: "100%",
      aspectRatio: "1",
      margin: "0 auto",
      borderRadius: "50%",
      background: `conic-gradient(${segments.map(({ color, start, end }) => `${color} ${start}% ${end}%`).join(", ")})`,
    },
  });
  const tooltip = appendElement(plotArea, "div", "", {
    attrs: { "aria-live": "polite" },
    style: {
      minHeight: "1.4rem",
      marginTop: "0.5rem",
      color: "var(--text-muted)",
      fontSize: "0.75rem",
      fontVariantNumeric: "tabular-nums",
      lineHeight: "1.4",
      opacity: "0",
      textAlign: "center",
      transition: "opacity 120ms ease-out",
    },
  });
  plot.addEventListener("mousemove", (event) => {
    const rect = plot.getBoundingClientRect();
    const x = event.clientX - rect.left - rect.width / 2;
    const y = event.clientY - rect.top - rect.height / 2;
    const radius = Math.min(rect.width, rect.height) / 2;
    if (Math.hypot(x, y) > radius) {
      showSliceDetails(tooltip, null, total, unit);
      return;
    }
    const angle = (Math.atan2(y, x) * 180 / Math.PI + 450) % 360;
    const position = angle / 3.6;
    const segment = segments.find(({ start, end }) => position >= start && position < end) ?? segments[segments.length - 1];
    showSliceDetails(tooltip, segment, total, unit);
  });
  plot.addEventListener("mouseleave", () => showSliceDetails(tooltip, null, total, unit));

  const legend = appendElement(chart, "div", "", {
    attrs: { role: "list", "aria-label": `${unit} distribution details` },
    style: { flex: "2 1 260px", minWidth: "0" },
  });
  appendElement(legend, "div", `${compact(total)} ${unit}`, {
    attrs: { title: `${total} ${unit}` },
    style: {
      marginBottom: "0.5rem",
      color: "var(--text-normal)",
      fontSize: "1.15rem",
      fontWeight: "650",
      fontVariantNumeric: "tabular-nums",
      lineHeight: "1.2",
    },
  });

  segments.forEach(({ label, value, color }) => {
    const share = percentage(value, total);
    const rowDetails = `${label}: ${compact(value)} ${unit}, ${share}`;
    const row = appendElement(legend, "div", "", {
      attrs: { role: "listitem", title: rowDetails, "aria-label": rowDetails },
      style: {
        display: "grid",
        gridTemplateColumns: "12px minmax(0, 1fr) auto",
        alignItems: "center",
        gap: "0.5rem",
        padding: "0.35rem 0",
      },
    });
    appendElement(row, "span", "", {
      style: { width: "12px", height: "12px", borderRadius: "2px", background: color },
    });
    appendElement(row, "span", label, {
      attrs: { title: label },
      style: {
        minWidth: "0",
        color: "var(--text-normal)",
        fontSize: "0.8rem",
        fontWeight: "600",
        lineHeight: "1.4",
        overflowWrap: "anywhere",
      },
    });
    appendElement(row, "span", `${compact(value)} · ${share}`, {
      style: {
        color: "var(--text-muted)",
        fontSize: "0.75rem",
        fontVariantNumeric: "tabular-nums",
        whiteSpace: "nowrap",
      },
    });
  });
}

try {
  const raw = await app.vault.adapter.read(snapshotPath);
  const data = JSON.parse(raw);
  const ranked = groupBy(data.usage_events ?? [], modelKey, (event) => tokenTotal(event.usage));
  const total = ranked.reduce((sum, [, value]) => sum + value, 0);
  const dark = document.body.classList.contains("theme-dark");
  const palette = dark
    ? ["#58a6ff", "#56d364", "#d2a8ff", "#f2cc60", "#ff7b72", "#39c5cf", "#ffa657", "#db61a2"]
    : ["#4e79a7", "#59a14f", "#af7aa1", "#edc948", "#e15759", "#76b7b2", "#f28e2b", "#ff9da7"];
  renderPieChart(ranked, total, "tokens", palette, dark ? "#8b949e" : "#bab0ab");
} catch (error) {
  dv.paragraph(`Could not read ${snapshotPath}: ${error.message}`);
}
```

## Top Projects

```dataviewjs
const snapshotPath = "__TOKENCHECK_SNAPSHOT_PATH__";
const configuredHomePath = "__TOKENCHECK_HOME_PATH__";
const minimumShare = 0.05;

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

function groupSmallSlices(items, total) {
  const slices = [];
  let otherValue = 0;
  for (const [label, value] of items) {
    if (total > 0 && value / total >= minimumShare) {
      slices.push([label, value]);
    } else {
      otherValue += value;
    }
  }
  if (otherValue > 0) slices.push(["Other", otherValue]);
  return slices;
}

function showSliceDetails(tooltip, segment, total, unit) {
  if (!segment) {
    tooltip.textContent = "";
    tooltip.style.opacity = "0";
    return;
  }
  tooltip.textContent = `${segment.label} · ${compact(segment.value)} ${unit} · ${percentage(segment.value, total)}`;
  tooltip.style.opacity = "1";
}

function renderPieChart(items, total, unit, palette, otherColor) {
  if (items.length === 0 || total <= 0) {
    dv.paragraph(`No ${unit} data found.`);
    return;
  }

  const slices = groupSmallSlices(items, total);
  let offset = 0;
  const segments = slices.map(([label, value], index) => {
    const start = offset;
    offset += (value / total) * 100;
    return {
      label,
      value,
      start,
      end: offset,
      color: label === "Other" ? otherColor : palette[index % palette.length],
    };
  });
  const details = segments
    .map(({ label, value }) => `${label}: ${compact(value)} ${unit}, ${percentage(value, total)}`)
    .join("; ");
  const chart = appendElement(dv.container, "div", "", {
    attrs: { role: "group", "aria-label": `${unit} distribution` },
    style: {
      display: "flex",
      flexWrap: "wrap",
      alignItems: "center",
      gap: "1.5rem",
      width: "100%",
      maxWidth: "100%",
      minWidth: "0",
      boxSizing: "border-box",
      margin: "0.5rem 0 1.5rem",
      padding: "1rem 0",
      borderTop: "1px solid var(--background-modifier-border)",
      borderBottom: "1px solid var(--background-modifier-border)",
    },
  });

  const plotArea = appendElement(chart, "div", "", {
    style: {
      display: "flex",
      flex: "1 1 220px",
      flexDirection: "column",
      alignItems: "center",
      minWidth: "0",
    },
  });
  const plot = appendElement(plotArea, "div", "", {
    attrs: { role: "img", "aria-label": details },
    style: {
      width: "220px",
      maxWidth: "100%",
      aspectRatio: "1",
      margin: "0 auto",
      borderRadius: "50%",
      background: `conic-gradient(${segments.map(({ color, start, end }) => `${color} ${start}% ${end}%`).join(", ")})`,
    },
  });
  const tooltip = appendElement(plotArea, "div", "", {
    attrs: { "aria-live": "polite" },
    style: {
      minHeight: "1.4rem",
      marginTop: "0.5rem",
      color: "var(--text-muted)",
      fontSize: "0.75rem",
      fontVariantNumeric: "tabular-nums",
      lineHeight: "1.4",
      opacity: "0",
      textAlign: "center",
      transition: "opacity 120ms ease-out",
    },
  });
  plot.addEventListener("mousemove", (event) => {
    const rect = plot.getBoundingClientRect();
    const x = event.clientX - rect.left - rect.width / 2;
    const y = event.clientY - rect.top - rect.height / 2;
    const radius = Math.min(rect.width, rect.height) / 2;
    if (Math.hypot(x, y) > radius) {
      showSliceDetails(tooltip, null, total, unit);
      return;
    }
    const angle = (Math.atan2(y, x) * 180 / Math.PI + 450) % 360;
    const position = angle / 3.6;
    const segment = segments.find(({ start, end }) => position >= start && position < end) ?? segments[segments.length - 1];
    showSliceDetails(tooltip, segment, total, unit);
  });
  plot.addEventListener("mouseleave", () => showSliceDetails(tooltip, null, total, unit));

  const legend = appendElement(chart, "div", "", {
    attrs: { role: "list", "aria-label": `${unit} distribution details` },
    style: { flex: "2 1 260px", minWidth: "0" },
  });
  appendElement(legend, "div", `${compact(total)} ${unit}`, {
    attrs: { title: `${total} ${unit}` },
    style: {
      marginBottom: "0.5rem",
      color: "var(--text-normal)",
      fontSize: "1.15rem",
      fontWeight: "650",
      fontVariantNumeric: "tabular-nums",
      lineHeight: "1.2",
    },
  });

  segments.forEach(({ label, value, color }) => {
    const share = percentage(value, total);
    const rowDetails = `${label}: ${compact(value)} ${unit}, ${share}`;
    const row = appendElement(legend, "div", "", {
      attrs: { role: "listitem", title: rowDetails, "aria-label": rowDetails },
      style: {
        display: "grid",
        gridTemplateColumns: "12px minmax(0, 1fr) auto",
        alignItems: "center",
        gap: "0.5rem",
        padding: "0.35rem 0",
      },
    });
    appendElement(row, "span", "", {
      style: { width: "12px", height: "12px", borderRadius: "2px", background: color },
    });
    appendElement(row, "span", label, {
      attrs: { title: label },
      style: {
        minWidth: "0",
        color: "var(--text-normal)",
        fontSize: "0.8rem",
        fontWeight: "600",
        lineHeight: "1.4",
        overflowWrap: "anywhere",
      },
    });
    appendElement(row, "span", `${compact(value)} · ${share}`, {
      style: {
        color: "var(--text-muted)",
        fontSize: "0.75rem",
        fontVariantNumeric: "tabular-nums",
        whiteSpace: "nowrap",
      },
    });
  });
}

try {
  const raw = await app.vault.adapter.read(snapshotPath);
  const data = JSON.parse(raw);
  const ranked = groupBy(data.usage_events ?? [], (event) => displayPath(event.project), (event) => tokenTotal(event.usage));
  const total = ranked.reduce((sum, [, value]) => sum + value, 0);
  const dark = document.body.classList.contains("theme-dark");
  const palette = dark
    ? ["#58a6ff", "#56d364", "#d2a8ff", "#f2cc60", "#ff7b72", "#39c5cf", "#ffa657", "#db61a2"]
    : ["#4e79a7", "#59a14f", "#af7aa1", "#edc948", "#e15759", "#76b7b2", "#f28e2b", "#ff9da7"];
  renderPieChart(ranked, total, "tokens", palette, dark ? "#8b949e" : "#bab0ab");
} catch (error) {
  dv.paragraph(`Could not read ${snapshotPath}: ${error.message}`);
}
```

## Top Tools

```dataviewjs
const snapshotPath = "__TOKENCHECK_SNAPSHOT_PATH__";
const minimumShare = 0.05;

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

function groupSmallSlices(items, total) {
  const slices = [];
  let otherValue = 0;
  for (const [label, value] of items) {
    if (total > 0 && value / total >= minimumShare) {
      slices.push([label, value]);
    } else {
      otherValue += value;
    }
  }
  if (otherValue > 0) slices.push(["Other", otherValue]);
  return slices;
}

function showSliceDetails(tooltip, segment, total, unit) {
  if (!segment) {
    tooltip.textContent = "";
    tooltip.style.opacity = "0";
    return;
  }
  tooltip.textContent = `${segment.label} · ${compact(segment.value)} ${unit} · ${percentage(segment.value, total)}`;
  tooltip.style.opacity = "1";
}

function renderPieChart(items, total, unit, palette, otherColor) {
  if (items.length === 0 || total <= 0) {
    dv.paragraph(`No ${unit} data found.`);
    return;
  }

  const slices = groupSmallSlices(items, total);
  let offset = 0;
  const segments = slices.map(([label, value], index) => {
    const start = offset;
    offset += (value / total) * 100;
    return {
      label,
      value,
      start,
      end: offset,
      color: label === "Other" ? otherColor : palette[index % palette.length],
    };
  });
  const details = segments
    .map(({ label, value }) => `${label}: ${compact(value)} ${unit}, ${percentage(value, total)}`)
    .join("; ");
  const chart = appendElement(dv.container, "div", "", {
    attrs: { role: "group", "aria-label": `${unit} distribution` },
    style: {
      display: "flex",
      flexWrap: "wrap",
      alignItems: "center",
      gap: "1.5rem",
      width: "100%",
      maxWidth: "100%",
      minWidth: "0",
      boxSizing: "border-box",
      margin: "0.5rem 0 1.5rem",
      padding: "1rem 0",
      borderTop: "1px solid var(--background-modifier-border)",
      borderBottom: "1px solid var(--background-modifier-border)",
    },
  });

  const plotArea = appendElement(chart, "div", "", {
    style: {
      display: "flex",
      flex: "1 1 220px",
      flexDirection: "column",
      alignItems: "center",
      minWidth: "0",
    },
  });
  const plot = appendElement(plotArea, "div", "", {
    attrs: { role: "img", "aria-label": details },
    style: {
      width: "220px",
      maxWidth: "100%",
      aspectRatio: "1",
      margin: "0 auto",
      borderRadius: "50%",
      background: `conic-gradient(${segments.map(({ color, start, end }) => `${color} ${start}% ${end}%`).join(", ")})`,
    },
  });
  const tooltip = appendElement(plotArea, "div", "", {
    attrs: { "aria-live": "polite" },
    style: {
      minHeight: "1.4rem",
      marginTop: "0.5rem",
      color: "var(--text-muted)",
      fontSize: "0.75rem",
      fontVariantNumeric: "tabular-nums",
      lineHeight: "1.4",
      opacity: "0",
      textAlign: "center",
      transition: "opacity 120ms ease-out",
    },
  });
  plot.addEventListener("mousemove", (event) => {
    const rect = plot.getBoundingClientRect();
    const x = event.clientX - rect.left - rect.width / 2;
    const y = event.clientY - rect.top - rect.height / 2;
    const radius = Math.min(rect.width, rect.height) / 2;
    if (Math.hypot(x, y) > radius) {
      showSliceDetails(tooltip, null, total, unit);
      return;
    }
    const angle = (Math.atan2(y, x) * 180 / Math.PI + 450) % 360;
    const position = angle / 3.6;
    const segment = segments.find(({ start, end }) => position >= start && position < end) ?? segments[segments.length - 1];
    showSliceDetails(tooltip, segment, total, unit);
  });
  plot.addEventListener("mouseleave", () => showSliceDetails(tooltip, null, total, unit));

  const legend = appendElement(chart, "div", "", {
    attrs: { role: "list", "aria-label": `${unit} distribution details` },
    style: { flex: "2 1 260px", minWidth: "0" },
  });
  appendElement(legend, "div", `${compact(total)} ${unit}`, {
    attrs: { title: `${total} ${unit}` },
    style: {
      marginBottom: "0.5rem",
      color: "var(--text-normal)",
      fontSize: "1.15rem",
      fontWeight: "650",
      fontVariantNumeric: "tabular-nums",
      lineHeight: "1.2",
    },
  });

  segments.forEach(({ label, value, color }) => {
    const share = percentage(value, total);
    const rowDetails = `${label}: ${compact(value)} ${unit}, ${share}`;
    const row = appendElement(legend, "div", "", {
      attrs: { role: "listitem", title: rowDetails, "aria-label": rowDetails },
      style: {
        display: "grid",
        gridTemplateColumns: "12px minmax(0, 1fr) auto",
        alignItems: "center",
        gap: "0.5rem",
        padding: "0.35rem 0",
      },
    });
    appendElement(row, "span", "", {
      style: { width: "12px", height: "12px", borderRadius: "2px", background: color },
    });
    appendElement(row, "span", label, {
      attrs: { title: label },
      style: {
        minWidth: "0",
        color: "var(--text-normal)",
        fontSize: "0.8rem",
        fontWeight: "600",
        lineHeight: "1.4",
        overflowWrap: "anywhere",
      },
    });
    appendElement(row, "span", `${compact(value)} · ${share}`, {
      style: {
        color: "var(--text-muted)",
        fontSize: "0.75rem",
        fontVariantNumeric: "tabular-nums",
        whiteSpace: "nowrap",
      },
    });
  });
}

try {
  const raw = await app.vault.adapter.read(snapshotPath);
  const data = JSON.parse(raw);
  const ranked = groupBy(data.tool_events ?? [], (event) => `${event.source}/${event.tool}`, () => 1);
  const total = ranked.reduce((sum, [, value]) => sum + value, 0);
  const dark = document.body.classList.contains("theme-dark");
  const palette = dark
    ? ["#58a6ff", "#56d364", "#d2a8ff", "#f2cc60", "#ff7b72", "#39c5cf", "#ffa657", "#db61a2"]
    : ["#4e79a7", "#59a14f", "#af7aa1", "#edc948", "#e15759", "#76b7b2", "#f28e2b", "#ff9da7"];
  renderPieChart(ranked, total, "calls", palette, dark ? "#8b949e" : "#bab0ab");
} catch (error) {
  dv.paragraph(`Could not read ${snapshotPath}: ${error.message}`);
}
```
