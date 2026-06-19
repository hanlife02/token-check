---
type: dashboard-section
source: __TOKENCHECK_SNAPSHOT_PATH__
section: summary
---

# Summary

__TOKENCHECK_DASHBOARD_LINK__

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
