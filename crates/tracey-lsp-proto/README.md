# tracey-lsp-proto

Minimal LSP prototype to test if completions and go-to-definition work in comments alongside rust-analyzer.

## Build

```bash
cargo build -p tracey-lsp-proto --release
```

## Test with Zed

Add to your Zed settings (`~/.config/zed/settings.json`):

```json
{
  "languages": {
    "Rust": {
      "language_servers": ["rust-analyzer", "tracey-lsp"]
    }
  },
  "lsp": {
    "tracey-lsp": {
      "binary": {
        "path": "/path/to/tracey/target/release/tracey-lsp-proto"
      }
    }
  }
}
```

Replace `/path/to/tracey` with the actual path to this repository.

## What to test

Open `test_file.rs` in Zed and try:

1. **Hover**: Place cursor on `auth.token.validation` in the comment - should show requirement description
2. **Go-to-definition**: Press `gd` on a requirement ID - should attempt to jump to spec file
3. **Completions**: Type `// r[` and see if verb completions appear (impl, verify, depends, related)
4. **Completions**: Type `// r[impl auth.` and see if requirement ID completions appear

## Debugging

Check Zed's LSP logs: `View > Toggle Log Pane` or `cmd+shift+p` -> "toggle log pane"

The LSP logs to stderr, which should appear in Zed's logs.
