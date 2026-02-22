+++
title = "Editor Integration"
+++

# Editor Integration

Tracey provides an LSP server that gives you real-time feedback on requirement references directly in your editor: diagnostics for broken references, hover to read requirement text, go-to-definition between code and spec, and completions for requirement IDs.

## How it works

`tracey lsp` starts a Language Server Protocol server over stdio. It connects to the tracey daemon (auto-starting it if needed) and uses a virtual filesystem overlay to track unsaved changes — you get instant feedback as you type, not just on save.

## Zed

The Tracey Zed extension is not yet published to the registry. To install it, open the command palette (`Cmd+Shift+P`), run `zed: install dev extension`, and select the `tracey-zed` directory from the tracey repository.

The extension activates for all supported source file types (`.rs`, `.ts`, `.tsx`, `.js`, `.jsx`, `.py`, `.go`, `.swift`, `.java`, `.md`) when a `.config/tracey/config.styx` file exists in the project.

## Other editors

Any editor with LSP support can use tracey. Configure your editor's LSP client to run:

```
tracey lsp
```

The server communicates over stdio. Register it for the file types you use (Rust, TypeScript, Python, Go, etc.) and for markdown files.

### VS Code

Add to `.vscode/settings.json` or configure via an LSP client extension. The exact setup depends on which LSP client extension you use. The language server command is `tracey lsp`.

### Neovim

With nvim-lspconfig or a similar plugin, configure a custom server:

```lua
vim.lsp.config['tracey'] = {
    cmd = { 'tracey', 'lsp' },
    filetypes = { 'rust', 'typescript', 'python', 'go', 'markdown' },
    root_markers = { '.config/tracey/config.styx' },
}
vim.lsp.enable('tracey')
```

## Features

### Diagnostics

Tracey reports problems directly in your editor:

- **Broken references** (Error) — `Unknown requirement 'auth.token.validaton'. Did you mean 'auth.token.validation'?`
- **Unknown prefix** (Error) — `Unknown prefix 'x'. Available prefixes: r, m, h2`
- **Unknown verb** (Warning) — unrecognized verb in a reference
- **Stale references** (Warning) — code references an older version of a requirement
- **Duplicate definitions** (Error) — same requirement ID defined twice in spec files
- **Impl in test file** (Error) — `impl` annotation in a file matched by `test_include`

Diagnostics update on save and are debounced during editing to avoid flicker.

### Hover

Hover over a requirement reference to see the full requirement text from the spec:

```
### auth.token.validation

The system must validate tokens before granting access.
Validation includes checking expiration, signature, and issuer.
```

The hover also shows the spec name and source URL (if configured). For versioned requirements, hover includes a diff showing what changed between versions.

### Go to definition

- **From a code reference** — jumps to the requirement definition in the spec markdown file
- **From a spec definition** — jumps to implementation references (shows a picker if multiple exist)

### Go to implementation

From any reference or definition, navigate to all `impl` references for that requirement. When multiple implementations exist, the editor shows a picker.

### Find references

Find all references to a requirement across the entire codebase — implementation, verification, dependency, and related references. Results are grouped by type.

### Completions

Type `r[` in a comment and tracey suggests matching requirement IDs with fuzzy matching. Typing `r[auth.tok` matches `auth.token.validation`. Each completion shows the requirement text in the detail popup.

Verb completions are also provided: after `r[` type a verb prefix and tracey suggests `impl`, `verify`, `depends`, `related`.

### Rename

Rename a requirement ID and tracey updates the definition in the spec file and all references across the codebase. The new ID is validated to ensure it follows naming conventions and doesn't conflict with existing requirements.

### Semantic tokens

Tracey provides semantic tokens for syntax highlighting: prefixes, verbs, and requirement IDs each get their own token type, allowing editors to apply distinct colors.

### Code lens

Requirement definitions in spec files can show inline coverage counts (e.g., "3 impls, 1 test") as code lens annotations.
