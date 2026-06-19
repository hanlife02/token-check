---
type: dashboard-section
source: __TOKENCHECK_SNAPSHOT_PATH__
section: recent-days
---

# Recent Days

__TOKENCHECK_DASHBOARD_LINK__

```dataviewjs
const snapshotPath = "__TOKENCHECK_SNAPSHOT_PATH__";
const number = new Intl.NumberFormat("en-US");

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
  const dailyTokens = groupBy(usageEvents, (event) => event.date, (event) => tokenTotal(event.usage));
  const dailySessions = new Map();

  for (const session of sessions) {
    if (!session.date || session.date === "unknown") continue;
    const ids = dailySessions.get(session.date) ?? new Set();
    ids.add(`${session.source}:${session.session_id}`);
    dailySessions.set(session.date, ids);
  }

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
} catch (error) {
  dv.paragraph(`Could not read ${snapshotPath}: ${error.message}`);
}
```
