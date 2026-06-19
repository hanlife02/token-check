# tokencheck

`tokencheck` 是一个本地优先的 Rust 命令行工具，用来统计本机 Claude Code、Codex 和 OpenCode 的 token 使用情况、工具调用、项目分布、模型分布和估算成本。

- crates.io 包名：`ethan-tkc`
- 安装后命令：`tkc`、`tokencheck`
- 当前版本：`0.12.0`
- License：MIT

当前版本优先读取本地日志和会话文件，只统计结构化元数据、token usage 和工具名；默认不会展示 prompt、回复正文、shell 命令参数、工具参数或文件快照内容。

## 目录

- [功能概览](#功能概览)
- [安装](#安装)
- [快速开始](#快速开始)
- [配置](#配置)
- [Codex Skill](#codex-skill)
- [Obsidian 集成](#obsidian-集成)
- [命令参考](#命令参考)
- [数据与快照](#数据与快照)
- [计费](#计费)
- [隐私边界](#隐私边界)
- [已知限制](#已知限制)
- [开发](#开发)
- [发布](#发布)
- [License](#license)

## 功能概览

- 统计 Claude Code、Codex 和 OpenCode 的总 token 使用量。
- 按日期、项目、模型和工具名聚合使用情况。
- 用 `tkc days --chant` 输出 Tokscale 风格终端柱状图。
- 用 `tkc heatmap` 输出终端周历热力图。
- 按模型价格估算每日、项目、模型和总美元成本。
- 支持自定义模型价格文件，覆盖或补充内置价格。
- 支持 JSON 快照，避免本机日志清理后丢失历史统计。
- 支持 `all`、`claude`、`codex`、`opencode` 四种数据来源过滤。
- 支持 `--since`、`--until` 按日期过滤报表。
- 支持 `tkc config` 交互式配置默认行为。
- 支持 `tkc doctor` 诊断配置、快照和本机数据目录。
- 支持 `tkc obsidian` 生成 Obsidian DataviewJS Dashboard。
- 支持英文和中文输出。

## 安装

### 从 crates.io 安装

需要本机已经安装 Rust 和 Cargo。

```bash
cargo install ethan-tkc
```

安装后可以使用两个等价入口：

```bash
tkc --help
tokencheck --help
tkc --version
```

### 更新

```bash
cargo install ethan-tkc --force
```

如果你之前用旧包名或本地路径安装过，可能会看到 `binary already exists`。先移除旧安装，再安装当前发布包：

```bash
cargo uninstall tokencheck
cargo install ethan-tkc
```

### 从源码运行

开发目录内临时运行：

```bash
cargo run --bin tkc -- summary
cargo run --bin tokencheck -- summary
```

开发目录内安装当前源码版本：

```bash
cargo install --path .
```

## 快速开始

查看总览。不传子命令时，默认等价于 `summary`：

```bash
tkc
tkc summary
```

保存或更新 JSON 快照：

```bash
tkc fetch
```

交互式配置默认行为：

```bash
tkc config
```

诊断当前配置、快照和本机数据目录：

```bash
tkc doctor
```

查看最近 10 天的表格统计：

```bash
tkc days --limit 10
```

查看最近日期的终端柱状图：

```bash
tkc days --chant --limit 30
```

查看最近 12 个月的终端热力图：

```bash
tkc heatmap
```

只看 Codex 数据：

```bash
tkc summary --source codex
tkc models --source codex
```

只从快照文件读取，不实时扫描 `$HOME`：

```bash
tkc summary --from-json
tkc days --from-json --limit 30
```

使用自定义快照文件：

```bash
tkc fetch --data-file data/workstation.json
tkc summary --from-json --data-file data/workstation.json
```

## 配置

`tkc config` 会在终端中一步一步询问配置项，并保存为 JSON 配置文件。每一步直接按 Enter 会保留当前显示的值；如果之前没有配置过，则使用内置默认值并保存。

```bash
tkc config
tkc config show
tkc config reset
```

默认配置文件位置：

```text
~/.config/tokencheck/config.json
```

如果设置了 `XDG_CONFIG_HOME`，配置会写入：

```text
$XDG_CONFIG_HOME/tokencheck/config.json
```

可配置项：

| 配置项 | 默认值 | 说明 |
| --- | --- | --- |
| `language` | `en` | 输出语言。支持 `en` 和 `zh`。 |
| `source` | `all` | 默认数据来源。支持 `all`、`claude`、`codex`、`opencode`。 |
| `data_file` | `data/tokencheck.json` | 默认快照文件路径，也就是 `fetch` 保存和 `--from-json` 读取的位置。 |
| `pricing_file` | 无 | 自定义价格 JSON 文件路径。输入 `none`、`null` 或 `-` 可以清空。 |
| `obsidian_snapshot_file` | 无 | Obsidian 使用的 JSON 快照路径。`tkc obsidian --fetch` 会写入这个文件；未配置时回退到 `data_file`。 |
| `obsidian_dashboard_file` | 无 | Obsidian Dashboard Markdown 文件路径。`tkc obsidian` 会创建或覆盖这个文件。 |
| `limit` | `20` | 默认表格或排行行数。 |
| `heatmap_months` | `12` | `heatmap` 默认展示月份数。 |

示例流程：

```text
tokencheck config
Config file: /Users/you/.config/tokencheck/config.json
Press Enter on a blank input to keep and save the shown value.

Language [en/zh] (current: en):
Default source [all/claude/codex/opencode] (current: all):
Snapshot data file (current: data/tokencheck.json): ~/.tokencheck/usage.json
Custom pricing file [path/none] (current: none): ~/.tokencheck/pricing.json
Obsidian snapshot file [path/none] (current: none): /Users/you/Vault/data/tokencheck.json
Obsidian dashboard file [path/none] (current: none): /Users/you/Vault/2 - Docs/token-check/Token Usage Dashboard.md
Default row limit (current: 20):
Default heatmap months (current: 12):

config saved: /Users/you/.config/tokencheck/config.json
```

命令行参数会覆盖 `tkc config` 保存的默认值：

```bash
tkc summary --source codex
tkc fetch --data-file data/one-off.json
tkc heatmap --months 6
```

## Codex Skill

本仓库附带一个 Codex Skill，用来让 Codex 更稳定地回答和操作 `tkc`。

```text
skills/tokencheck/
├── SKILL.md
└── agents/openai.yaml
```

如果你只是使用工具，不需要 clone 本仓库。先安装 CLI：

```bash
cargo install ethan-tkc
```

然后通过 Codex 自带的 skill installer 只安装 Codex Skill：

```bash
python3 ~/.codex/skills/.system/skill-installer/scripts/install-skill-from-github.py \
  --repo hanlife02/token-check \
  --path skills/tokencheck
```

如果本机已经存在旧版 skill，installer 会拒绝覆盖。可以先备份旧目录，再重新安装：

```bash
mv ~/.codex/skills/tokencheck ~/.codex/skills/tokencheck.bak
python3 ~/.codex/skills/.system/skill-installer/scripts/install-skill-from-github.py \
  --repo hanlife02/token-check \
  --path skills/tokencheck
```

安装后重启 Codex，并用下面的提示触发：

```text
Use $tokencheck to configure tkc and troubleshoot local token usage reports.
```

验证本仓库内的 skill 文件：

```bash
python3 ~/.codex/skills/.system/skill-creator/scripts/quick_validate.py skills/tokencheck
```

验证已经安装到本机 Codex 的 skill：

```bash
python3 ~/.codex/skills/.system/skill-creator/scripts/quick_validate.py ~/.codex/skills/tokencheck
```

如果你正在开发或审查本项目，也可以从仓库根目录复制 skill 到 Codex skill 目录：

```bash
mkdir -p ~/.codex/skills
cp -R skills/tokencheck ~/.codex/skills/
```

## Obsidian 集成

`tkc obsidian` 会根据配置生成 Obsidian DataviewJS 报表笔记。它使用两个配置项：

| 配置项 | 用途 |
| --- | --- |
| `obsidian_snapshot_file` | DataviewJS 读取的 JSON 数据源。 |
| `obsidian_dashboard_file` | 要创建或更新的 Markdown 报表笔记。 |

示例配置：

```text
Obsidian snapshot file [path/none] (current: none): /Users/you/Vault/data/tokencheck.json
Obsidian dashboard file [path/none] (current: none): /Users/you/Vault/2 - Docs/token-check/Token Usage Dashboard.md
```

生成 Dashboard：

```bash
tkc obsidian
```

生成 Dashboard 并先更新 Obsidian 快照：

```bash
tkc obsidian --fetch
```

临时覆盖路径：

```bash
tkc obsidian \
  --snapshot-file "/Users/you/Vault/data/tokencheck.json" \
  --dashboard-file "/Users/you/Vault/2 - Docs/token-check/Token Usage Dashboard.md"
```

生成的 Dashboard 主文件是轻量索引页，并会在同目录生成 Summary、Recent Days、Top Models、Top Projects 和 Top Tools 分块页面。每个分块页面使用 DataviewJS 读取 vault 内的 `data/tokencheck.json`，打开对应页面时才渲染对应报表。Obsidian 侧需要安装并启用 Dataview 插件，同时打开 Dataview 的 JavaScript 查询功能。

JSON 通常作为数据源使用，不需要直接在 Obsidian 中阅读。如果想让 Obsidian 文件列表显示 JSON，可以启用 `Files and links -> Detect all file extensions`。

## 命令参考

### 命令一览

| 命令 | 作用 | 常用场景 |
| --- | --- | --- |
| `tkc` | 默认执行 `summary` | 快速查看总览 |
| `tkc config` | 交互式配置默认行为 | 设置语言、快照路径、默认来源和默认显示数量 |
| `tkc config show` | 查看当前生效配置 | 检查配置文件路径和默认值 |
| `tkc config reset` | 删除配置文件并恢复默认值 | 清空本机自定义配置 |
| `tkc doctor` | 诊断配置、快照和本机数据目录 | 排查安装后没有数据、路径不对或快照不可读 |
| `tkc obsidian` | 生成 Obsidian DataviewJS Dashboard | 在 Obsidian 中查看 token 用量报表 |
| `tkc fetch` | 扫描本机数据并合并写入 JSON 快照 | 保存历史、换机器前备份、日志清理前归档 |
| `tkc summary` | 输出整体统计和分来源统计 | 查看总 token、session、工具调用和估算成本 |
| `tkc days` | 按日期聚合 token 和成本 | 看最近哪些天用量最高 |
| `tkc days --chant` | 用 Tokscale 风格柱状图展示每日 token | 在终端中快速看趋势 |
| `tkc heatmap` | 用周历热力图展示每日 token 强度 | 看长期使用节奏 |
| `tkc projects` | 按项目路径聚合 token 和成本 | 找出最耗 token 的项目 |
| `tkc models` | 按模型聚合 token 和成本 | 分析模型使用和成本结构 |
| `tkc tools` | 按工具名统计调用次数 | 查看 Read、Edit、Bash 等工具的使用频率 |

### 全局参数

这些参数可以放在子命令前后：

```bash
tkc --source codex summary
tkc summary --source codex
```

| 参数 | 默认值 | 说明 |
| --- | --- | --- |
| `--source all\|claude\|codex\|opencode` | 配置值，初始为 `all` | 选择数据来源。`all` 同时统计 Claude Code、Codex 和 OpenCode。 |
| `--home <PATH>` | 当前 `$HOME` | 指定要扫描的 home 目录。适合测试、读取备份目录或统计另一份用户数据。 |
| `--limit <N>` | 配置值，初始为 `20` | 控制排行或日期输出数量。主要影响 `days`、`projects`、`models`、`tools`。 |
| `--from-json` | 关闭 | 只读取 JSON 快照，不扫描实时本机日志。对 `fetch` 无效。 |
| `--data-file <PATH>` | 配置值，初始为 `data/tokencheck.json` | 指定 `fetch` 写入或 `--from-json` 读取的快照文件。 |
| `--pricing-file <PATH>` | 配置值，初始为无 | 指定自定义模型价格 JSON 文件。自定义价格会覆盖同名内置价格。 |
| `--since <DATE>` | 无 | 只统计该日期及之后的数据。支持 `YYYY-MM-DD`、`today`、`7d`、`30d`。 |
| `--until <DATE>` | 无 | 只统计该日期及之前的数据。支持 `YYYY-MM-DD`、`today`、`7d`、`30d`。 |
| `-h`, `--help` | - | 打印命令帮助。 |

`--home` 只影响实时扫描。使用 `--from-json` 时，命令只读取 `--data-file`，不会访问 `$HOME`。

`--since` 和 `--until` 只影响报表命令，不影响 `fetch` 写入快照，也不影响 `doctor` 诊断；这样快照会保持完整，报表可以按需切片查看。过滤区间是闭区间，包含起止日期。

日期过滤示例：

```bash
tkc summary --since 2026-05-01
tkc days --since 30d
tkc models --since 7d --until today
tkc projects --from-json --since 2026-01-01 --until 2026-03-31
```

相对日期按当前日期向前计算，例如 `7d` 表示 7 天前的日期。

### `tkc` / `tkc summary`

`tkc` 不带子命令时等价于：

```bash
tkc summary
```

`summary` 输出整体统计和按来源拆分的表格。它适合回答这些问题：

- 一共扫描到了多少 session？
- 多少 session 有 token usage？
- 涉及多少项目和模型？
- 总 token 是多少？
- 估算成本是多少？
- Claude Code、Codex 和 OpenCode 分别贡献了多少输入、缓存、输出和工具调用？

示例：

```bash
tkc summary
tkc summary --source claude
tkc summary --source codex
tkc summary --source opencode
tkc summary --from-json
tkc summary --since 30d
```

输出顶部字段：

| 字段 | 含义 |
| --- | --- |
| `sessions scanned` | 扫描到并去重后的 session 数量。 |
| `sessions with usage` | 至少包含一次 token usage 的 session 数量。 |
| `projects seen` | 出现过的项目路径数量，不含 `unknown`。 |
| `models seen` | 出现过的模型数量，不含 `unknown`。 |
| `usage events` | 参与聚合的 usage 事件数量。 |
| `tool calls` | 解析到的工具调用事件数量。 |
| `total tokens` | input、cache、output、reasoning 等字段合并后的总 token。 |
| `estimated cost` | 按内置价格表估算的美元成本。 |

### `tkc fetch`

`fetch` 扫描本机 Claude Code、Codex 和 OpenCode 数据，并把结果合并保存到 JSON 快照。默认写入：

```text
data/tokencheck.json
```

示例：

```bash
tkc fetch
tkc fetch --source codex
tkc fetch --source opencode
tkc fetch --source claude --data-file data/claude.json
tkc fetch --home /Users/yourname --data-file data/backup.json
```

`fetch` 的合并策略是保守的：

- 新 session 会追加。
- 新 usage event 会追加。
- 新 tool event 会追加。
- 同一个 usage event 只有在新扫描的 token 总量更大时才升级。
- 如果新扫描结果缺少旧快照中已有的数据，旧数据会保留。
- 如果没有新增或升级内容，已有 JSON 文件不会被重写。

`fetch` 输出的计数含义：

| 输出 | 含义 |
| --- | --- |
| `snapshot saved` | 快照被创建或更新。 |
| `snapshot unchanged` | 扫描后没有新增或升级内容。 |
| `sessions: A -> B` | 合并前后 session 数量。 |
| `usage events: A -> B (+N, upgraded M)` | 新增 usage 事件数量和升级事件数量。 |
| `tool calls: A -> B (+N)` | 新增工具调用事件数量。 |
| `total tokens: A -> B` | 合并前后的总 token。 |

### `tkc days`

`days` 按日期聚合 token 使用量和估算成本，默认显示最近 20 个有 usage 的日期，最新日期在前。

```bash
tkc days
tkc days --limit 7
tkc days --source codex --limit 30
tkc days --from-json --limit 90
tkc days --since 2026-05-01 --until today
```

表格字段：

| 字段 | 含义 |
| --- | --- |
| `date` | 日期，格式为 `YYYY-MM-DD`。 |
| `sessions` | 当天涉及的 session 数。 |
| `input` | 普通输入 token。 |
| `cached` | cache read / cached input token。 |
| `cache_create` | Claude cache write / cache creation token。 |
| `output` | 输出 token。 |
| `reasoning` | Codex reasoning output token。 |
| `total` | 当天总 token。 |
| `cost` | 当天估算成本。 |

### `tkc days --chant`

`days --chant` 使用 Tokscale 风格的圆角面板和竖直彩色柱状图展示每日 total tokens。

```bash
tkc days --chant
tkc days --chant --limit 60
tkc days --chant --source claude
```

图表会根据终端宽度自动裁剪可显示日期数量：

- 终端足够宽时，显示 `--limit` 指定的全部日期。
- 终端较窄时，只显示能放下的最近日期，并在面板顶部显示 `可见天数/请求天数`。
- 终端过窄时，显示 `Terminal is too narrow for the chart`，避免边框和内容错位。

图表底部依次显示日期、token 数和成本。颜色强度和 `heatmap` 使用同一套等级。

### `tkc heatmap`

`heatmap` 按日聚合 total tokens，并输出周历热力图。横向是周，纵向是星期，颜色越亮表示当天 token 越多。

```bash
tkc heatmap
tkc heatmap --months 6
tkc heatmap --months 24 --from-json
tkc heatmap --source codex
```

规则：

- 默认展示配置中的 `heatmap_months`，初始为最近 12 个月。
- `--months <N>` 控制月份跨度，最小按 1 个月处理。
- 时间范围以数据中的最新日期为结束月份，不一定是今天。
- 色阶按当前可见范围内的最大日用量归一化。
- 终端较窄时会自动裁剪为能显示的最近周数。
- 终端过窄时，显示 `Terminal is too narrow for the heatmap`。

`heatmap` 只展示 token 强度，不展示成本。如果要看每日成本，用 `tkc days`。

### `tkc projects`

`projects` 按 `source + project path` 聚合 usage，按 total tokens 从高到低排序。

```bash
tkc projects
tkc projects --limit 10
tkc projects --source claude
tkc projects --from-json --data-file data/tokencheck.json
tkc projects --since 30d
```

这个命令适合找出最耗 token 的项目。输出字段和 `days` 类似，但第一列是 `source`，第二列是 `project`。

注意：

- Claude Code 的项目通常来自 `~/.claude/projects` 下的项目路径映射。
- Codex 的项目来自 session 记录中的工作目录信息。
- 如果原始数据缺少项目路径，会显示为 `unknown`。

### `tkc models`

`models` 按 `source + model` 聚合 usage，按 total tokens 从高到低排序。

```bash
tkc models
tkc models --limit 20
tkc models --source codex
tkc models --from-json
tkc models --since 7d
```

这个命令适合分析哪些模型用得最多、哪些模型贡献了主要成本，以及是否还有未配置价格的模型名。

如果某个模型没有内置价格，`cost` 会带 `*`，并在命令结束后输出 warning。带 `*` 的成本是部分成本，不包含未定价模型。

### `tkc tools`

`tools` 按 `source + tool name` 统计工具调用次数，按调用次数从高到低排序。

```bash
tkc tools
tkc tools --limit 50
tkc tools --source claude
tkc tools --from-json
tkc tools --since 2026-05-01
```

输出字段：

| 字段 | 含义 |
| --- | --- |
| `source` | `Claude` 或 `Codex`。 |
| `tool` | 工具名，例如 `Read`、`Edit`、`Bash`、`apply_patch`。 |
| `calls` | 工具调用次数。 |
| `days` | 该工具出现过的日期数量。 |
| `projects` | 该工具出现过的项目数量。 |

`tools` 不输出 token 或成本，因为工具调用事件本身只记录工具元数据；token 用量来自 usage events。

### `tkc doctor`

`doctor` 会检查当前环境是否能正常产出报表。它不会写入快照，也不会修改配置。

```bash
tkc doctor
tkc doctor --source codex
tkc doctor --home /Users/yourname
tkc doctor --data-file data/workstation.json
tkc doctor --pricing-file ~/.tokencheck/pricing.json
```

诊断项包括：

| 检查项 | 含义 |
| --- | --- |
| `Config file` | 当前配置文件路径是否存在；不存在时会使用内置默认值。 |
| `Home directory` | 实时扫描使用的 home 目录是否存在。 |
| `Source filter` | 当前生效的数据来源过滤：`all`、`claude`、`codex` 或 `opencode`。 |
| `Custom pricing file` | 自定义价格文件是否存在且能解析；未配置时使用内置价格。 |
| `Claude Code data directory` | 是否能找到 `$HOME/.claude/projects`。 |
| `Codex data directory` | 是否能找到 `$HOME/.codex/sessions`。 |
| `OpenCode data directory` | 是否能找到 `$HOME/.local/share/opencode`。 |
| `Snapshot file` | 配置或命令行指定的 JSON 快照是否存在且可读取。 |
| `Live scan` | 实时扫描当前本机日志后得到的 session、usage、tool 和 token 计数。 |
| `Report data` | 当前配置下是否已经有可用 usage 数据。 |

如果你安装后运行 `tkc` 没有数据，优先运行 `tkc doctor`。常见原因是 Claude/Codex/OpenCode 日志目录不存在、`--home` 指错目录、只配置了某一个 `source`，或者快照文件还没有通过 `tkc fetch` 创建。

## 数据与快照

### 工作方式

报表命令默认会做三件事：

1. 读取 `tkc config` 保存的配置；如果没有配置文件，则使用内置默认值。
2. 扫描当前用户 `$HOME` 下的 Claude Code、Codex 和 OpenCode 本地数据。
3. 如果配置的快照文件存在，默认是 `data/tokencheck.json`，把快照数据和实时扫描结果合并后展示。

这样可以保留历史统计：即使 Claude Code、Codex 或 OpenCode 的原始日志后续被清理，只要你之前运行过 `tkc fetch`，快照中的旧数据仍然会参与报表。

```text
Claude/Codex JSONL + OpenCode SQLite/JSON -> collector -> ReportData
data/tokencheck.json -> snapshot loader -> ReportData
ReportData -> source filter -> command aggregation -> terminal output
```

`fetch` 是唯一会写入快照的命令。其他报表命令只读取和展示数据。

### 数据来源

Claude Code：

```text
~/.claude/projects/**/*.jsonl
```

Codex：

```text
~/.codex/sessions/**/*.jsonl
```

OpenCode：

```text
~/.local/share/opencode/opencode.db
~/.local/share/opencode/storage/message/**/*.json
~/.local/share/opencode/storage/session/**/*.json
```

当前版本不默认扫描 `~/.codex/log/codex-tui.log`，因为该文件可能非常大，且包含更细的运行日志。

### JSON 快照

默认快照路径来自 `tkc config` 的 `data_file`，初始值是：

```text
data/tokencheck.json
```

推荐用法：

```bash
tkc fetch
tkc summary --from-json
tkc days --from-json --limit 30
```

快照适合这些场景：

- 本机日志可能被清理，但你想保留历史统计。
- 想把多个时间点的扫描结果累计在同一个 JSON 文件中。
- 想在不扫描 `$HOME` 的情况下查看报表。
- 想把统计数据带到另一台机器上查看。

报表命令默认会合并实时扫描和快照。要只看快照，必须加 `--from-json`。要永久修改快照位置，运行 `tkc config` 并设置 `Snapshot data file`；要临时覆盖，使用 `--data-file <PATH>`。

### 输出字段和统计口径

常见 token 字段：

| 字段 | 说明 |
| --- | --- |
| `input` | 普通输入 token。 |
| `cached` | 已缓存输入 token。Claude 侧对应 cache read；Codex 侧对应 cached input。 |
| `cache_create` | Claude cache creation / cache write token。 |
| `output` | 模型输出 token。 |
| `reasoning` | Codex reasoning output token。 |
| `total` | 工具按可用字段计算出的总 token。 |
| `cost` | 按内置价格估算的美元成本。 |

Claude Code：

- 递归扫描 `projects` 下的 JSONL 文件，包括 subagents。
- session 数按 `sessionId` 去重。
- token usage 按 `sessionId + message.id` 去重；没有 `message.id` 时使用 fallback key。
- 统计字段包括 input、cached input、cache creation input、output。

Codex：

- 递归扫描 `sessions` 下的 JSONL 文件。
- 每个 session 只取最后一条非空累计 `token_count.info.total_token_usage`。
- 统计字段包括 input、cached input、output、reasoning output、total。

## 计费

- `summary`、`days`、`projects` 和 `models` 会输出 `cost`，单位是美元。
- Codex 数据里的 cached input 视为 input 的子集，不重复计入普通 input。
- Claude Code 数据里的 cache read/cache write 按独立字段计费。
- Claude 模型按 input、cache read、5 分钟 cache write、1 小时 cache write 和 output 分别计费。
- 其他文本模型按 input、cached input/cache hit 和 output 估算。
- 当前内置价格覆盖常见 OpenAI GPT/o 系列、Claude Opus/Sonnet/Haiku、Gemini 3/2.5/2.0、DeepSeek V4、MiMo V2/V2.5、Kimi K2/Moonshot V1 官方模型名和常见 snapshot 名。
- 如果设置了自定义价格文件，同名自定义价格会优先于内置价格。
- 如果模型没有内置价格，也没有自定义价格，成本显示会带 `*`，并在 warning 中说明该模型未计入美元总额。
- 成本只按文本 token 估算，不包含订阅费、Batch/Flex/Priority 折扣、工具调用附加费、图片/音频/视频单独计费、税费或第三方代理加价。

### 自定义模型价格

自定义价格文件是一个 JSON 文件，单位是每 100 万 token 的美元价格。最小格式如下：

```json
{
  "models": {
    "my-proxy-model": {
      "input": 2.0,
      "cached_input": 0.5,
      "cache_creation_5m": 3.0,
      "cache_creation_1h": 4.0,
      "output": 8.0
    }
  }
}
```

字段说明：

| 字段 | 必填 | 说明 |
| --- | --- | --- |
| `input` | 是 | 普通 input token 单价。 |
| `output` | 是 | output token 单价。 |
| `cached_input` | 否 | cached input/cache read 单价；省略时等于 `input`。 |
| `cache_creation_5m` | 否 | Claude 5 分钟 cache write 单价；省略时为 `0`。 |
| `cache_creation_1h` | 否 | Claude 1 小时 cache write 单价；省略时为 `0`。 |

配置方式：

```bash
tkc config
tkc summary --pricing-file ~/.tokencheck/pricing.json
tkc models --pricing-file ~/.tokencheck/pricing.json
```

模型名会按和内置价格相同的规则规范化：大小写不敏感，`_` 会当成 `-`，`provider/model` 会按最后一段模型名匹配。因此 `openai/gpt-4.1-mini` 和 `gpt-4.1-mini` 会匹配同一条价格。自定义价格会覆盖同名内置价格。

## 隐私边界

默认不会输出：

- 用户 prompt 正文。
- assistant 回复正文。
- shell 命令参数。
- 工具调用参数。
- 文件快照内容。
- paste-cache 内容。

当前命令只展示聚合后的元数据和计数。`fetch` 保存的 JSON 快照也用于报表聚合；它不是原始对话导出。

## 已知限制

- 模型价格是内置静态配置，供应商价格变化后需要更新代码。
- 成本是按文本 token 的估算值，不等同于实际账单。
- 第三方代理模型名可能不等同于官方模型名，只能按已知别名匹配。
- `heatmap` 只展示 token 强度，不展示成本。

## 开发

### 项目结构

```text
src/
├── billing.rs       # 模型价格匹配、cache 计费口径、成本估算
├── claude_code/     # Claude Code JSONL 数据读取
├── codex/           # Codex sessions JSONL 数据读取
├── opencode/        # OpenCode SQLite 与 JSON 数据读取
├── lib.rs           # CLI、命令分发、聚合、终端渲染
├── model.rs         # 共享数据模型
├── snapshot.rs      # JSON 快照读写、去重、合并
└── bin/
    ├── tkc.rs
    └── tokencheck.rs
```

核心数据流是：读取本机日志或 JSON 快照，归一化为 `ReportData`，按命令维度聚合，再输出表格、柱状图、热力图或成本估算。

### 开发验证

提交前建议运行：

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
```

本地冒烟检查：

```bash
cargo run --bin tkc -- summary --limit 5
cargo run --bin tkc -- days --limit 5
cargo run --bin tkc -- days --from-json --limit 20 --chant
cargo run --bin tkc -- heatmap --from-json --months 12
cargo run --bin tkc -- projects --limit 5
cargo run --bin tkc -- models --limit 5
cargo run --bin tkc -- tools --limit 5
```

## 发布

项目包含两个 GitHub Actions workflows：

```text
.github/workflows/cargo-publish.yml  # 发布 crates.io 包
.github/workflows/release.yml        # 构建并发布 GitHub Release 二进制包
```

### crates.io 发布

触发方式：

- 推送到 `main`，且本项目源码、manifest、README、LICENSE 或 `cargo-publish.yml` 有变化。
- 手动运行 `workflow_dispatch`。

发布步骤：

1. 读取 `Cargo.toml` 中的 package name 和 version。
2. 运行 `cargo fmt --all -- --check`。
3. 运行 `cargo clippy --all-targets -- -D warnings`。
4. 运行 `cargo test`。
5. 运行 `cargo package`。
6. 查询 crates.io sparse index 是否已有相同 package/version。
7. 如果版本尚未发布，则执行 `cargo publish`。
8. 如果版本已经存在，则跳过发布。

首次启用前，需要在 GitHub 仓库的 Actions secrets 中配置：

```text
CARGO_REGISTRY_TOKEN
```

这个 token 来自 crates.io 账户设置。crates.io 账户必须有已验证邮箱。之后每次要发布新包，需要先提升 `Cargo.toml` 和 `Cargo.lock` 中的版本号，再推送到 `main`。crates.io 不允许覆盖已经发布过的同一版本。

### GitHub Release 二进制包

触发方式：

- 推送 `v*` tag，例如 `v0.12.0`。
- 手动运行 `Build GitHub release` workflow。未填写 tag 时，默认使用 `Cargo.toml` 版本生成 `v<version>`。

release workflow 会先运行 `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings` 和 `cargo test`，再为以下平台构建并上传归档：

| 平台 | target | 归档 |
| --- | --- | --- |
| Linux x64 | `x86_64-unknown-linux-gnu` | `.tar.gz` |
| Windows x64 | `x86_64-pc-windows-msvc` | `.zip` |
| macOS Intel | `x86_64-apple-darwin` | `.tar.gz` |
| macOS Apple Silicon | `aarch64-apple-darwin` | `.tar.gz` |

每个归档内包含 `tkc`、`tokencheck`、`README.md` 和 `LICENSE`。workflow 还会上传 `SHA256SUMS` 校验文件。GitHub Release 使用仓库内置的 `GITHUB_TOKEN`，不需要额外配置 secret。

## License

MIT. See [LICENSE](LICENSE).
