# tokencheck

`tokencheck` 是一个本地命令行工具，用来统计本机 Claude Code 和 Codex 的使用情况。短命令名是 `tkc`。

当前版本优先读取本地日志和会话文件，只统计结构化元数据、token usage 和工具名；默认不会展示 prompt、回复正文、shell 命令参数、工具参数或文件快照内容。

## 功能

- 统计 Claude Code 和 Codex 的总使用量。
- 按日期查看 token 趋势。
- 按项目路径查看使用排行。
- 按模型查看 token 分布。
- 按工具名查看调用次数。
- 支持只看 Claude Code、只看 Codex，或同时统计两者。

## 安装

在项目根目录执行：

```bash
cargo install --path .
```

安装后可以使用：

```bash
tkc --help
tokencheck --help
```

如果只想在开发目录临时运行：

```bash
cargo run --bin tkc -- summary
```

## 快速开始

查看总览：

```bash
tkc summary
```

查看最近日期排行：

```bash
tkc days --limit 10
```

查看项目使用排行：

```bash
tkc projects --limit 10
```

查看模型使用排行：

```bash
tkc models --limit 10
```

查看工具调用排行：

```bash
tkc tools --limit 10
```

不传子命令时，默认等价于：

```bash
tkc summary
```

## 命令说明

### `summary`

输出整体统计：

- sessions scanned：扫描到的会话数。
- sessions with usage：包含 token usage 的会话数。
- projects seen：涉及的项目路径数量。
- models seen：涉及的模型数量。
- usage events：参与聚合的 usage 事件数。
- tool calls：工具调用次数。
- total tokens：总 token。

示例：

```bash
tkc summary
tkc summary --source codex
tkc summary --source claude
```

### `days`

按日期聚合 token 使用量。

```bash
tkc days
tkc days --limit 30
```

### `projects`

按项目路径聚合 token 使用量。

```bash
tkc projects
tkc projects --source codex --limit 20
```

### `models`

按来源和模型聚合 token 使用量。

```bash
tkc models
tkc models --source claude
```

### `tools`

按来源和工具名统计调用次数。

```bash
tkc tools
tkc tools --limit 50
```

输出中的 `days` 和 `projects` 表示该工具出现过的日期数和项目数。

## 全局参数

### `--source`

选择数据来源：

```bash
tkc summary --source all
tkc summary --source claude
tkc summary --source codex
```

默认值是 `all`。

### `--limit`

控制排行输出行数：

```bash
tkc projects --limit 5
tkc tools --limit 100
```

默认值是 `20`。

### `--home`

指定要读取的 home 目录。默认读取当前用户的 `$HOME`。

```bash
tkc summary --home /Users/yourname
```

这个参数适合测试、迁移数据或读取备份目录。

## 数据来源

Claude Code：

```text
~/.claude/projects/**/*.jsonl
```

Codex：

```text
~/.codex/sessions/**/*.jsonl
```

当前版本不默认扫描 `~/.codex/log/codex-tui.log`，因为该文件可能非常大，且包含更细的运行日志。

## 统计口径

Claude Code：

- 递归扫描 `projects` 下的 JSONL 文件，包括 subagents。
- session 数按 `sessionId` 去重。
- token usage 按 `sessionId + message.id` 去重；没有 `message.id` 时使用 fallback key。
- 统计字段包括 input、cached input、cache creation input、output。

Codex：

- 递归扫描 `sessions` 下的 JSONL 文件。
- 每个 session 只取最后一条非空累计 `token_count.info.total_token_usage`。
- 统计字段包括 input、cached input、output、reasoning output、total。

## 隐私边界

默认不会输出：

- 用户 prompt 正文。
- assistant 回复正文。
- shell 命令参数。
- 工具调用参数。
- 文件快照内容。
- paste-cache 内容。

当前命令只展示聚合后的元数据和计数。

## 开发验证

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
cargo run --bin tkc -- projects --limit 5
cargo run --bin tkc -- models --limit 5
cargo run --bin tkc -- tools --limit 5
```
