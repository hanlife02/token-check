---
type: dashboard
source: __TOKENCHECK_SNAPSHOT_PATH__
---

# Token Usage Dashboard

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
    [...sessions, ...usageEvents].map((item) => item.model).filter((value) => value && value !== "unknown")
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
      ["Snapshot", snapshotPath],
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
  const recentDays = dailyTokens.sort((a, b) => b[0].localeCompare(a[0])).slice(0, 30);
  const maxDaily = Math.max(...recentDays.map(([, tokens]) => tokens), 0);
  dv.header(2, "Recent Days");
  dv.table(
    ["Date", "Sessions", "Tokens", "Trend"],
    recentDays.map(([date, tokens]) => [
      date,
      number.format(dailySessions.get(date)?.size ?? 0),
      compact(tokens),
      bar(tokens, maxDaily),
    ])
  );

  const topModels = groupBy(
    usageEvents,
    (event) => `${event.source}/${event.model}`,
    (event) => tokenTotal(event.usage)
  ).slice(0, 15);
  const maxModel = Math.max(...topModels.map(([, tokens]) => tokens), 0);
  dv.header(2, "Top Models");
  dv.table(["Model", "Tokens", "Share"], topModels.map(([model, tokens]) => [model, compact(tokens), bar(tokens, maxModel)]));

  const topProjects = groupBy(usageEvents, (event) => event.project, (event) => tokenTotal(event.usage)).slice(0, 15);
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
