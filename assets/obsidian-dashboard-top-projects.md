---
type: dashboard-section
source: __TOKENCHECK_SNAPSHOT_PATH__
section: top-projects
---

# Top Projects

__TOKENCHECK_DASHBOARD_LINK__

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
  return new Intl.NumberFormat("en-US").format(value);
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
  const usageEvents = data.usage_events ?? [];
  const topProjects = groupBy(usageEvents, (event) => displayPath(event.project), (event) => tokenTotal(event.usage)).slice(0, 15);
  const maxProject = Math.max(...topProjects.map(([, tokens]) => tokens), 0);

  dv.table(["Project", "Tokens", "Share"], topProjects.map(([project, tokens]) => [project, compact(tokens), bar(tokens, maxProject)]));
} catch (error) {
  dv.paragraph(`Could not read ${snapshotPath}: ${error.message}`);
}
```
