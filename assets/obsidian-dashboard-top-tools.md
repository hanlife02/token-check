---
type: dashboard-section
source: __TOKENCHECK_SNAPSHOT_PATH__
section: top-tools
---

# Top Tools

__TOKENCHECK_DASHBOARD_LINK__

```dataviewjs
const snapshotPath = "__TOKENCHECK_SNAPSHOT_PATH__";
const number = new Intl.NumberFormat("en-US");

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
  const toolEvents = data.tool_events ?? [];
  const topTools = groupBy(toolEvents, (event) => `${event.source}/${event.tool}`, () => 1).slice(0, 15);
  const maxTool = Math.max(...topTools.map(([, calls]) => calls), 0);

  dv.table(["Tool", "Calls", "Share"], topTools.map(([tool, calls]) => [tool, number.format(calls), bar(calls, maxTool)]));
} catch (error) {
  dv.paragraph(`Could not read ${snapshotPath}: ${error.message}`);
}
```
