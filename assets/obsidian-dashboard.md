---
type: dashboard
source: __TOKENCHECK_SNAPSHOT_PATH__
---

# Token Usage Dashboard

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

function appendElement(parent, tag, text = "", options = {}) {
  const element = document.createElement(tag);
  if (text) element.textContent = text;
  if (options.className) element.className = options.className;
  if (options.attrs) {
    for (const [name, value] of Object.entries(options.attrs)) {
      element.setAttribute(name, value);
    }
  }
  if (options.style) Object.assign(element.style, options.style);
  parent.appendChild(element);
  return element;
}

function renderTable(parent, headers, rows) {
  parent.replaceChildren();
  const table = appendElement(parent, "table", "", {
    className: "dataview table-view-table",
    style: { width: "100%" },
  });
  const thead = appendElement(table, "thead");
  const headerRow = appendElement(thead, "tr");
  for (const header of headers) appendElement(headerRow, "th", header);
  const tbody = appendElement(table, "tbody");
  for (const row of rows) {
    const tr = appendElement(tbody, "tr");
    for (const cell of row) appendElement(tr, "td", String(cell ?? ""));
  }
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

function bar(value, max, width = 18) {
  if (!max) return "";
  const filled = Math.max(1, Math.round((value / max) * width));
  return "█".repeat(filled) + "░".repeat(Math.max(0, width - filled));
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

  dv.header(2, "Summary");
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

  const dailyTokens = groupBy(usageEvents, (event) => event.date, (event) => tokenTotal(event.usage));
  const dailySessions = new Map();
  for (const session of sessions) {
    if (!session.date || session.date === "unknown") continue;
    const ids = dailySessions.get(session.date) ?? new Set();
    ids.add(`${session.source}:${session.session_id}`);
    dailySessions.set(session.date, ids);
  }
  dv.header(2, "Recent Days");
  const sortedDays = dailyTokens.sort((a, b) => a[0].localeCompare(b[0]));
  if (sortedDays.length === 0) {
    dv.paragraph("No daily usage found.");
  } else {
    const firstDate = sortedDays[0][0];
    const lastDate = sortedDays[sortedDays.length - 1][0];
    const defaultStart = sortedDays[Math.max(0, sortedDays.length - 30)][0];
    const dailySection = appendElement(dv.container, "div");
    const controls = appendElement(dailySection, "div", "", {
      style: { display: "flex", flexWrap: "wrap", gap: "0.75rem", alignItems: "center", margin: "0.5rem 0 1rem" },
    });
    const startLabel = appendElement(controls, "label", "Start", {
      style: { display: "flex", gap: "0.35rem", alignItems: "center" },
    });
    const startInput = appendElement(startLabel, "input", "", { attrs: { type: "date", min: firstDate, max: lastDate } });
    startInput.value = defaultStart;
    const endLabel = appendElement(controls, "label", "End", {
      style: { display: "flex", gap: "0.35rem", alignItems: "center" },
    });
    const endInput = appendElement(endLabel, "input", "", { attrs: { type: "date", min: firstDate, max: lastDate } });
    endInput.value = lastDate;
    const rangeTotal = appendElement(controls, "span", "", {
      style: { fontWeight: "600" },
    });
    const dailyTable = appendElement(dailySection, "div");
    const renderDaily = () => {
      const start = startInput.value || firstDate;
      const end = endInput.value || lastDate;
      if (start > end) {
        rangeTotal.textContent = "Range total: -";
        dailyTable.replaceChildren();
        appendElement(dailyTable, "p", "Start date must be on or before end date.");
        return;
      }
      const selectedDays = sortedDays
        .filter(([date]) => date >= start && date <= end)
        .sort((a, b) => b[0].localeCompare(a[0]));
      const rangeTokens = selectedDays.reduce((sum, [, tokens]) => sum + tokens, 0);
      rangeTotal.textContent = `Range total: ${compact(rangeTokens)}`;
      const maxDaily = Math.max(...selectedDays.map(([, tokens]) => tokens), 0);
      renderTable(
        dailyTable,
        ["Date", "Sessions", "Tokens", "Trend"],
        selectedDays.map(([date, tokens]) => [
          date,
          number.format(dailySessions.get(date)?.size ?? 0),
          compact(tokens),
          bar(tokens, maxDaily),
        ])
      );
    };
    startInput.addEventListener("change", renderDaily);
    endInput.addEventListener("change", renderDaily);
    renderDaily();
  }

  const topModels = groupBy(
    usageEvents,
    modelKey,
    (event) => tokenTotal(event.usage)
  ).slice(0, 15);
  const maxModel = Math.max(...topModels.map(([, tokens]) => tokens), 0);
  dv.header(2, "Top Models");
  dv.table(["Model", "Tokens", "Share"], topModels.map(([model, tokens]) => [model, compact(tokens), bar(tokens, maxModel)]));

  const topProjects = groupBy(usageEvents, (event) => displayPath(event.project), (event) => tokenTotal(event.usage)).slice(0, 15);
  const maxProject = Math.max(...topProjects.map(([, tokens]) => tokens), 0);
  dv.header(2, "Top Projects");
  dv.table(["Project", "Tokens", "Share"], topProjects.map(([project, tokens]) => [project, compact(tokens), bar(tokens, maxProject)]));

  const topTools = groupBy(toolEvents, (event) => `${event.source}/${event.tool}`, () => 1).slice(0, 15);
  const maxTool = Math.max(...topTools.map(([, calls]) => calls), 0);
  dv.header(2, "Top Tools");
  dv.table(["Tool", "Calls", "Share"], topTools.map(([tool, calls]) => [tool, number.format(calls), bar(calls, maxTool)]));
} catch (error) {
  dv.paragraph(`Could not read ${snapshotPath}: ${error.message}`);
}
```
