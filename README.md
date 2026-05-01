# token-check

`token-check` 是一个基于 [tokscale](https://www.npmjs.com/package/tokscale) 的本地 Token / 成本归档与终端看板项目。它每天导出模型使用数据，保留按日期归档的 JSON 快照，并通过 Rich 在终端里展示模型、月份、统计概览和贡献热力图。

## 功能概览

- 每日导出 `tokscale` 的三类数据：模型维度、月份维度、图表/统计维度。
- 自动保存到 `data/YYYYMMDD/`，并同步覆盖 `data/latest/` 作为最新数据入口。
- 提供终端 Dashboard，展示 Models、Monthly、Stats Overview 和 Contribution Heatmap。
- 保留历史 JSON 快照，便于后续做趋势分析、归档或导入其他工具。

## 项目结构

```text
.
├── data/
│   ├── YYYYMMDD/
│   │   ├── graph.json
│   │   ├── models.json
│   │   └── monthly.json
│   └── latest/
│       ├── graph.json
│       ├── models.json
│       └── monthly.json
├── scripts/
│   ├── daily-export.sh
│   └── dashboard.py
├── tokscale-graph.json
└── tokscale-models.json
```

说明：

- `data/YYYYMMDD/`：当天的完整导出快照。
- `data/latest/`：最近一次成功导出的数据，看板默认读取这里。
- `scripts/daily-export.sh`：每日自动导出脚本。
- `scripts/dashboard.py`：Rich 终端看板。
- `tokscale-*.json`：早期手动导出的根目录快照，主要用于留档。

## 环境要求

- Python 3.9+
- Node.js / npm / npx
- Python 依赖：`rich`
- 可正常执行的 `npx tokscale@latest`

安装 Python 依赖：

```bash
python3 -m pip install rich
```

## 快速开始

导出最新数据：

```bash
bash scripts/daily-export.sh
```

查看终端看板：

```bash
python3 scripts/dashboard.py
```

如果当前项目不在 `~/code/token-check`，建议显式传入数据目录：

```bash
python3 scripts/dashboard.py --dir data/latest
```

## 数据导出流程

`scripts/daily-export.sh` 会依次执行：

```bash
npx tokscale@latest --json
npx tokscale@latest monthly --json
npx tokscale@latest graph --output graph.json
```

成功后会写入：

- `data/YYYYMMDD/models.json`
- `data/YYYYMMDD/monthly.json`
- `data/YYYYMMDD/graph.json`
- `data/latest/models.json`
- `data/latest/monthly.json`
- `data/latest/graph.json`

脚本具备当天幂等逻辑：如果当天的 `models.json` 已存在且非空，会直接跳过，避免重复导出。

## 自动化建议

可以用 `cron` 定时运行每日导出：

```cron
5 0 * * * /Users/hanlife02/code/token-check/scripts/daily-export.sh >> /tmp/token-check-export.log 2>&1
```

也可以根据需要改成 LaunchAgent。无论使用哪种方式，都要确保执行环境中能找到 `node`、`npm`、`npx`，以及 `tokscale` 所需的本地配置。

## Dashboard 说明

默认命令：

```bash
python3 scripts/dashboard.py
```

默认读取：

```text
~/code/token-check/data/latest
```

可通过 `--dir` 指定任意一次历史快照：

```bash
python3 scripts/dashboard.py --dir data/20260430
```

看板包含：

- `Models`：按 client + model 聚合输入、输出、缓存、消息数和成本。
- `Monthly`：按月份聚合模型使用和成本。
- `Stats Overview`：总 token、总成本、活跃天数、峰值日成本、客户端和模型列表。
- `Contribution Heatmap`：按日期成本渲染的 GitHub 风格终端热力图。

## 常见问题

### 提示找不到数据目录

先运行导出脚本：

```bash
bash scripts/daily-export.sh
```

或者显式指定现有数据目录：

```bash
python3 scripts/dashboard.py --dir data/latest
```

### `ModuleNotFoundError: No module named 'rich'`

安装 Rich：

```bash
python3 -m pip install rich
```

### `npx tokscale@latest` 执行失败

检查：

- Node.js / npm / npx 是否可用。
- 当前机器是否能访问 npm registry。
- `tokscale` 能否在当前用户环境中读取到需要统计的数据源。

## 维护提示

- `daily-export.sh` 中的 `OUTPUT_DIR` 当前固定为 `$HOME/code/token-check`。
- `dashboard.py` 的默认数据目录当前固定为 `~/code/token-check/data/latest`。
- 如果迁移项目路径，需要同步调整上述默认路径，或运行看板时始终传入 `--dir`。
