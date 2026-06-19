---
type: dashboard-section
source: __TOKENCHECK_SNAPSHOT_PATH__
section: top-models
---

# Top Models

__TOKENCHECK_DASHBOARD_LINK__

```dataviewjs
const snapshotPath = "__TOKENCHECK_SNAPSHOT_PATH__";

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
  const topModels = groupBy(
    usageEvents,
    modelKey,
    (event) => tokenTotal(event.usage)
  ).slice(0, 15);
  const maxModel = Math.max(...topModels.map(([, tokens]) => tokens), 0);

  dv.table(["Model", "Tokens", "Share"], topModels.map(([model, tokens]) => [model, compact(tokens), bar(tokens, maxModel)]));
} catch (error) {
  dv.paragraph(`Could not read ${snapshotPath}: ${error.message}`);
}
```
