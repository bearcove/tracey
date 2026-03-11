# tracey-zed

A [Zed](https://zed.dev) extension for [tracey](https://github.com/bearcove/tracey), providing requirement traceability features in your editor.

## Features

- **Diagnostics**: Errors for broken requirement references and unknown prefixes
- **Hover**: View requirement text and coverage info
- **Go to Definition**: Jump from reference to spec definition
- **Completions**: Suggest requirement IDs when typing `r[...]`
- **Inlay Hints**: Coverage status shown inline
- **Code Lens**: Implementation and test counts on requirement definitions
- **Rename**: Rename requirement IDs across spec and implementation files

- **MCP Context Server**: Exposes tracey's query tools (status, uncovered,
  untested, stale, unmapped, rule, config, reload, validate) to Zed's AI
  assistant

## Installation

### Option 1: Automatic (Recommended)

Install this extension from the Zed extension registry (search for "Tracey"). The extension will automatically download the tracey binary from GitHub releases.

### Option 2: Manual Binary Installation

If you prefer to manage the binary yourself:

1. Install the tracey binary:
   ```bash
   cargo binstall tracey
   # or: cargo install tracey
   # or: download from https://github.com/bearcove/tracey/releases
   ```

2. Install this extension from the Zed extension registry

The extension will use the downloaded binary if available, otherwise fall back to looking for `tracey` in your PATH.

## Configuration

The extension uses tracey's standard configuration at `.config/tracey/config.styx` in your project root.

### Enabling the MCP Context Server

The LSP features work out of the box, but the MCP context server must be
enabled in your Zed settings. Open your settings (`Cmd+,` on macOS, `Ctrl+,`
on Linux) and add:

```json
{
  "context_servers": {
    "tracey": {
      "enabled": true,
      "settings": {}
    }
  }
}
```

Once enabled, tracey's tools will be available to the AI assistant panel
(`Cmd+Shift+A` / `Ctrl+Shift+A`). You can ask the assistant to check coverage
status, find uncovered rules, inspect specific requirements, and more.

## Supported Languages

Rust, TypeScript, TSX, JavaScript, Python, Go, and Markdown.

## Requirements

- Zed editor
- A tracey configuration file in your project
