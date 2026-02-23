# tracey

> **Note:** Looking for Tracy, the frame profiler? That's a different project: [wolfpld/tracy](https://github.com/wolfpld/tracy)

Spec coverage for codebases. Tracks traceability between requirements (in markdown) and implementations/tests (in source code). Catches spec drift before it becomes a problem.

## What it does

Specs, implementations, and tests drift apart — code changes without updating specs, specs describe unimplemented features, tests cover different scenarios than requirements specify.

Tracey uses lightweight annotations in markdown and source code comments to link specification requirements with implementing code and tests. This enables:

- Verifying multiple implementations (different languages, platforms) match the same spec
- Finding which requirements lack implementation or tests
- Seeing which requirement justifies each piece of code
- Analyzing impact when requirements or code changes
- Detecting stale references when spec text changes but code annotations haven't been updated

For the full specification, see [docs/spec/tracey.md](docs/spec/tracey.md).

## Installation

```bash
# Pre-built binary (fast, from GitHub Releases)
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/bearcove/tracey/releases/latest/download/tracey-installer.sh | sh

# Or build from source (main branch)
cargo install --locked --git https://github.com/bearcove/tracey --branch main tracey
```

Pre-built binaries are available for `aarch64-apple-darwin`, `aarch64-unknown-linux-gnu`, `x86_64-unknown-linux-gnu`, `x86_64-pc-windows-msvc`, and `aarch64-pc-windows-msvc`.

## Quick Start

### 1. Define requirements in your spec (markdown)

Use the `r[req.id]` syntax to define requirements in your specification documents:

```markdown
# Channel Management

r[channel.id.allocation]
Channel IDs MUST be allocated sequentially starting from 0.

r[channel.id.parity]
Client-initiated channels MUST use odd IDs, server-initiated channels MUST use even IDs.
```

The prefix (`r` in this case) can be any lowercase alphanumeric marker. Tracey infers it from the spec files.

### 2. Reference requirements in your code

Add references in source code comments using `PREFIX[VERB REQ]`:

```rust
// r[impl channel.id.allocation]
fn allocate_channel_id(&mut self) -> u32 {
    let id = self.next_id;
    self.next_id += 1;
    id
}

// r[impl channel.id.parity]
fn next_client_channel(&mut self) -> u32 {
    // ...
}
```

```rust
// In test files:
// r[verify channel.id.parity]
#[test]
fn client_channels_are_odd() {
    // ...
}
```

Verbs:

| Verb | Meaning |
|------|---------|
| `impl` | This code implements the requirement (default if verb omitted) |
| `verify` | This code tests/verifies the requirement |
| `depends` | This code depends on the requirement |
| `related` | This code is related to the requirement |

### 3. Configure tracey

Create `.config/tracey/config.styx`:

```styx
specs (
  {
    name my-spec
    include (docs/spec/**/*.md)
    impls (
      {
        name rust
        include (src/**/*.rs)
        exclude (target/**)
        test_include (tests/**/*.rs)
      }
    )
  }
)
```

Config fields:

| Field | Description |
|-------|-------------|
| `name` | Display name for the spec or implementation |
| `include` | Glob patterns for files to scan |
| `exclude` | Glob patterns for files to skip |
| `test_include` | Glob patterns for test files (only `verify` annotations allowed) |
| `source_url` | Canonical URL for the spec (e.g. a GitHub repository) |

### 4. Launch the dashboard

```bash
tracey web
# or: tracey web --open  (opens browser automatically)
```

## Architecture

Tracey runs as a persistent daemon per workspace. All interfaces (web dashboard, LSP, MCP, CLI queries) connect to the daemon over a Unix socket using [roam](https://github.com/bearcove/roam) RPC.

```
                    .tracey/daemon.sock
                            │
             ┌──────────────┼──────────────┐
             ▼              ▼              ▼
         HTTP bridge    MCP bridge     LSP bridge
         (dashboard)    (stdio)        (tower-lsp)
```

The daemon watches the filesystem, rebuilds on changes (debounced), and auto-exits after 10 minutes of inactivity. All bridges auto-start the daemon if it isn't running.

## Interfaces

### Web Dashboard (`tracey web`)

Interactive browser UI with three views:

- **Spec view** — rendered spec with inline requirement status, click-through to implementations
- **Coverage view** — filterable table of all requirements with impl/verify coverage
- **Sources view** — file tree with coverage badges, syntax-highlighted source with annotations

Supports Cmd+K / Ctrl+K search across all requirements.

### LSP (`tracey lsp`)

Full language server with:

- Hover info showing requirement text and coverage status
- Go-to-definition (jump from code reference to spec) and find-all-references
- Diagnostics for broken references, unknown prefixes, stale annotations
- Completions for requirement IDs and verbs
- Rename support (rename a requirement ID across all files)
- Code lens, inlay hints, semantic tokens

Install the [Zed extension](tracey-zed/) or point any LSP-compatible editor at `tracey lsp`.

### MCP Server (`tracey mcp`)

Exposes tracey as an [MCP](https://modelcontextprotocol.io/) tool server for AI assistants. Tools include `tracey_status`, `tracey_uncovered`, `tracey_untested`, `tracey_stale`, `tracey_unmapped`, `tracey_rule`, `tracey_config`, `tracey_validate`, and more.

### CLI Queries (`tracey query`)

Same queries available from the terminal:

```bash
tracey query status              # coverage overview
tracey query uncovered           # rules with no impl references
tracey query untested            # rules with impl but no verify references
tracey query stale               # references pointing to older rule versions
tracey query unmapped            # source tree with coverage percentages
tracey query rule auth.login     # full details for a specific rule
tracey query validate            # check for broken refs, naming issues
```

### AI Skill (`tracey skill install`)

Bundled skill for Claude Code and Codex that teaches the AI how to add correct tracey annotations:

```bash
tracey skill install --claude    # install to ~/.claude/skills/tracey
tracey skill install --codex     # install to ~/.codex/skills/tracey
```

### Git Hooks

```bash
tracey pre-commit   # fail if rule text changed without a version bump
tracey bump         # auto-bump version numbers of changed rules, re-stage
```

## Version Tracking

Requirements support version suffixes for tracking spec evolution:

```markdown
> r[auth.login+3]
> Users MUST authenticate with a valid token.
```

In code, references include the version they were written against:

```rust
// r[impl auth.login+3]
```

When spec text changes and the version is bumped to `+4`, tracey reports the `+3` reference as **stale** — the code needs review to confirm it still matches the updated requirement. `tracey bump` automates the version bumping for staged changes.

## Supported Languages

Tracey scans comments in: Rust, Swift, TypeScript, TSX, JavaScript, JSX, Go, C, C++, Objective-C, Objective-C++, Java, Kotlin, Scala, Groovy, C#, Zig, PHP.

## License

[MIT](LICENSE-MIT) OR [Apache-2.0](LICENSE-APACHE)
