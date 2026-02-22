+++
title = "CLI Reference"
weight = 9
+++

All tracey commands. Each command accepts an optional `[ROOT]` argument to specify the project root directory (defaults to the current directory).

## Dashboard and servers

### `tracey web`

Start the web dashboard.

```
tracey web [--port PORT] [--open] [--config PATH] [ROOT]
```

| Flag | Description |
|------|-------------|
| `-p, --port` | Port to listen on (default: 3000) |
| `--open` | Open the dashboard in your browser |
| `-c, --config` | Config file path (default: `.config/tracey/config.styx`) |

Auto-starts the daemon if it isn't running.

### `tracey lsp`

Start the LSP server for editor integration. Typically not run manually.

```
tracey lsp [--config PATH] [ROOT]
```

Communicates over stdio. See [Editor Integration](editor-integration.md) for setup.

### `tracey mcp`

Start the MCP server for AI assistants.

```
tracey mcp [--config PATH] [ROOT]
```

Communicates over stdio. See [AI Integration](ai-integration.md) for setup.

## Daemon management

Tracey uses a persistent daemon process per workspace. All bridges (web, LSP, MCP, CLI) connect to the daemon as clients. The daemon is auto-started by bridges, so you rarely need to manage it directly.

### `tracey daemon`

Start the daemon in the foreground.

```
tracey daemon [--config PATH] [ROOT]
```

Writes `.tracey/daemon.pid` (contains PID and wire protocol version). Logs to `.tracey/daemon.log`. Managed by `tracey kill`.

### `tracey status`

Show daemon status including uptime, watcher state, and data version.

```
tracey status [ROOT]
```

### `tracey logs`

Show daemon log output.

```
tracey logs [--follow] [--lines N] [ROOT]
```

| Flag | Description |
|------|-------------|
| `-f, --follow` | Stream new log entries (like `tail -f`) |
| `-n, --lines` | Number of historical lines to show (default: 50) |

### `tracey kill`

Stop the running daemon and clean up stale sockets.

```
tracey kill [ROOT]
```

## Terminal queries

Query coverage data from the terminal. These commands connect to the daemon (auto-starting it if needed).

### `tracey query status`

Coverage overview showing percentages for all spec/implementation pairs.

```
tracey query status [ROOT]
```

### `tracey query uncovered`

List requirements without `impl` references, grouped by spec section.

```
tracey query uncovered [--spec_impl SPEC/IMPL] [--prefix PREFIX] [ROOT]
```

### `tracey query untested`

List requirements without `verify` references.

```
tracey query untested [--spec_impl SPEC/IMPL] [--prefix PREFIX] [ROOT]
```

### `tracey query stale`

List references pointing to older rule versions.

```
tracey query stale [--spec_impl SPEC/IMPL] [--prefix PREFIX] [ROOT]
```

### `tracey query unmapped`

Show source tree with coverage percentages. Code units (functions, structs, etc.) without requirement references are "unmapped."

```
tracey query unmapped [--spec_impl SPEC/IMPL] [--path PATH] [ROOT]
```

Pass `--path` to zoom into a specific directory or file and see individual unmapped code units.

### `tracey query rule`

Show full details about a specific rule: its text, where it's defined, and all implementation/verification references.

```
tracey query rule RULE_ID [ROOT]
```

### `tracey query config`

Display the current configuration.

```
tracey query config [ROOT]
```

### `tracey query validate`

Run all validation checks: broken references, naming violations, circular dependencies, orphaned requirements, duplicates, stale references.

```
tracey query validate [--spec_impl SPEC/IMPL] [ROOT]
```

## Spec versioning

### `tracey pre-commit`

Check staged spec files for requirements whose text changed without a version bump. Exits with an error if any are found. Designed to be used as a git pre-commit hook.

```
tracey pre-commit [--config PATH] [ROOT]
```

### `tracey bump`

Auto-bump version numbers of staged requirements whose text changed, then re-stage the modified files.

```
tracey bump [--config PATH] [ROOT]
```

See [Versioning](versioning.md) for the full workflow.

## AI skill management

### `tracey skill install`

Install the bundled Tracey skill for AI assistants.

```
tracey skill install [--claude] [--codex]
```

| Flag | Description |
|------|-------------|
| `--claude` | Install only for Claude Code (`~/.claude/skills/tracey`) |
| `--codex` | Install only for Codex CLI (`~/.codex/skills/tracey`) |

Installs for both by default if neither flag is given.

## Shell completions

Generate shell completion scripts:

```bash
tracey --completions bash   # Bash
tracey --completions zsh    # Zsh
tracey --completions fish   # Fish
```

## Gitignore

Add `.tracey/` to your `.gitignore` — it contains the daemon socket and log file:

```
# .gitignore
.tracey/
```
