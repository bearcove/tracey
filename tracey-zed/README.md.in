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

### Dev extension

The Tracey Zed extension is not available in the registry at this time. You have to install it yourself by opening the command palette (Cmd+Shift+P on mac, Ctrl+Shift+P elsewhere) and picking "install dev extension", then choosing `path/to/tracey/tracey-zed`.

It will compile the tree-sitter grammar and the extension to WASM. If everything goes fine, Tracey should show up in your installed extensions. If it doesn't, you can show "zed: open log" from the command palette to see what went wrong.

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
