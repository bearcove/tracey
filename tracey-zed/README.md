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

The extension uses tracey's standard configuration at `.config/tracey/config.kdl` in your project root.

## Supported Languages

Rust, TypeScript, TSX, JavaScript, Python, Go, and Markdown.

## Requirements

- Zed editor
- A tracey configuration file in your project
