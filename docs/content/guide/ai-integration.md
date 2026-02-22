+++
title = "AI Integration"
weight = 7
+++

Tracey exposes its coverage analysis as MCP (Model Context Protocol) tools, letting AI assistants like Claude Code query requirements, find uncovered code, and add annotations.

## Setup

### Option 1: MCP config

Add a `.mcp.json` file to your project root:

```json
{
    "mcpServers": {
        "tracey": {
            "command": "tracey",
            "args": ["mcp"]
        }
    }
}
```

The MCP server auto-starts the tracey daemon when it connects.

### Option 2: Install the skill

```bash
tracey skill install
```

This installs a skill file that teaches AI assistants how to use tracey's tools effectively. By default it installs for both Claude Code and Codex. Use `--claude` or `--codex` to install for just one.

## Available tools

| Tool | Purpose |
|------|---------|
| `tracey_status` | Coverage overview — shows configured specs, prefixes, and percentages |
| `tracey_uncovered` | Requirements without `impl` references |
| `tracey_untested` | Requirements without `verify` references |
| `tracey_stale` | References pointing to older rule versions |
| `tracey_unmapped` | Source tree with coverage — shows code without requirement references |
| `tracey_rule` | Full details about a specific requirement |
| `tracey_config` | Display current configuration |
| `tracey_validate` | Check for broken references, naming issues, duplicates |
| `tracey_reload` | Reload config and rebuild data |
| `tracey_config_include` | Add an include pattern to an implementation |
| `tracey_config_exclude` | Add an exclude pattern to an implementation |

### Filtering

`tracey_uncovered`, `tracey_untested`, and `tracey_stale` accept optional parameters:

- `spec_impl` — filter to a specific spec/implementation (e.g., `"myapp/rust"`)
- `prefix` — filter by requirement ID prefix (e.g., `"auth."` to see only auth requirements)

`tracey_unmapped` accepts an optional `path` parameter to zoom into a directory or file.

## Workflow

A typical session with an AI assistant:

1. **Check coverage** — the assistant calls `tracey_status` to see what specs exist, what prefix to use, and current coverage percentages.

2. **Find work** — `tracey_uncovered` shows requirements that lack implementations. `tracey_untested` shows requirements without tests.

3. **Read requirements** — `tracey_rule` fetches the full text of a specific requirement, along with all existing references.

4. **Annotate code** — the assistant adds `r[impl req.id]` or `r[verify req.id]` comments to the appropriate code.

5. **Verify** — `tracey_status` again confirms coverage improved.

### Example conversation

> **You:** "Add spec annotations to the auth module"
>
> The assistant calls `tracey_status`, sees prefix `r` and 45% coverage, then calls `tracey_uncovered` to find `auth.login`, `auth.token-expiry`, and `auth.rate-limiting` are uncovered. It reads each requirement with `tracey_rule`, finds the implementing functions, and adds the annotations.

## Response format

MCP tool responses include:

- **Status header** — current coverage for all spec/implementation pairs
- **Delta** — what changed since the last query (newly covered requirements)
- **Hints** — suggestions for what to query next

Responses are formatted as human-readable text, not JSON.

## Single vs. multiple specs

When only one spec and one implementation are configured, tools use them by default — no need to specify `spec_impl`. When multiple exist, tools either auto-detect from context or ask you to specify.
