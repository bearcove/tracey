# tracey

> **Note:** Looking for Tracy, the frame profiler? That's a different project: [wolfpld/tracy](https://github.com/wolfpld/tracy)

A CLI tool and library to measure spec coverage in codebases, with an interactive dashboard for exploring traceability.

## What it does

tracey parses source files to find references to specification rules (in the format `[rule.id]` in comments) and compares them against a spec manifest to produce coverage reports.

### Supported Languages

tracey works with any language that uses `//` or `/* */` comment syntax:

- **Rust** (.rs)
- **Swift** (.swift)
- **TypeScript/JavaScript** (.ts, .tsx, .js, .jsx)
- **Go** (.go)
- **C/C++** (.c, .h, .cpp, .hpp, .cc, .cxx)
- **Objective-C** (.m, .mm)
- **Java** (.java)
- **Kotlin** (.kt, .kts)
- **Scala** (.scala)
- **C#** (.cs)
- **Zig** (.zig)

This enables **traceability** between your spec documents and implementation code.

## Installation

```bash
# With cargo-binstall (fast, downloads pre-built binary)
cargo binstall tracey

# Or build from source
cargo install tracey
```

## Quick Start

### 1. Define rules in your spec (markdown)

Use the `r[rule.id]` syntax to define rules in your specification documents:

```markdown
# Channel Management

r[channel.id.allocation]
Channel IDs MUST be allocated sequentially starting from 0.

r[channel.id.parity]
Client-initiated channels MUST use odd IDs, server-initiated channels MUST use even IDs.
```

Rules can include metadata attributes:

```markdown
r[channel.id.allocation status=stable level=must since=1.0]
Channel IDs MUST be allocated sequentially starting from 0.

r[experimental.feature status=draft]
This feature is under development.

r[old.behavior status=deprecated until=3.0]
This behavior is deprecated and will be removed.
```

Supported attributes:
- `status`: `draft`, `stable`, `deprecated`, `removed`
- `level`: `must`, `should`, `may` (RFC 2119)
- `since`: version when introduced
- `until`: version when deprecated/removed
- `tags`: comma-separated custom tags

### 2. Reference rules in your code

Reference spec rules in comments. The syntax works across all supported languages:

**Rust:**
```rust
/// Allocates the next channel ID for this peer.
/// [impl channel.id.parity]
fn allocate_channel_id(&mut self) -> u32 { ... }
```

**Swift:**
```swift
/// Allocates the next channel ID for this peer.
/// [impl channel.id.parity]
func allocateChannelId() -> UInt32 { ... }
```

**TypeScript:**
```typescript
/**
 * Allocates the next channel ID for this peer.
 * [impl channel.id.parity]
 */
function allocateChannelId(): number { ... }
```

### 3. Configure tracey

Create `.config/tracey/config.kdl`:

```kdl
spec {
    name "my-spec"
    rules_glob "docs/spec/**/*.md"
}
```

### 4. Run coverage check

```bash
tracey
```

Output:

```
-> Extracting rules for my-spec from markdown files matching docs/spec/**/*.md...
   Found 2 rules from docs/spec/channels.md
   Found 2 rules in spec
-> Scanning source files...
   Found 2 rule references

## my-spec Coverage Report

Coverage: 100.0% (2/2 rules)
  References: 2 impl
```

## Rule Reference Syntax

tracey recognizes rule references in comments with optional verbs:

| Syntax | Description |
|--------|-------------|
| `[impl rule.id]` | Implementation of the rule |
| `[verify rule.id]` | Test/verification of the rule |
| `[define rule.id]` | Definition point for the rule |
| `[depends rule.id]` | Code depends on this rule's guarantees |
| `[related rule.id]` | Related but not direct implementation |
| `[rule.id]` | Basic reference (legacy, treated as impl) |

The verb helps categorize references in the coverage report and traceability matrix.

## Configuration Options

### Loading rules from markdown files (recommended)

Point tracey directly at your spec markdown files:

```kdl
spec {
    name "my-spec"
    rules_glob "docs/spec/**/*.md"
}
```

tracey will extract `r[rule.id]` markers and use them as the rule manifest.

### Loading rules from a JSON manifest

If you have a pre-generated `_rules.json` file:

```kdl
spec {
    name "my-spec"
    rules_file "path/to/_rules.json"
}
```

### Loading rules from a URL

For specs hosted online:

```kdl
spec {
    name "my-spec"
    rules_url "https://example.com/_rules.json"
}
```

### Filtering source files

Control which source files are scanned. By default, tracey scans all supported file types. You can restrict to specific patterns:

```kdl
spec {
    name "my-spec"
    rules_glob "docs/**/*.md"
    
    // Rust only
    include "src/**/*.rs"
    include "crates/**/*.rs"
    exclude "target/**"
}
```

For a multi-language project:

```kdl
spec {
    name "my-protocol"
    rules_glob "docs/**/*.md"
    
    // Scan Rust, Swift, and TypeScript
    include "crates/**/*.rs"
    include "Sources/**/*.swift"
    include "src/**/*.ts"
    include "src/**/*.tsx"
    exclude "node_modules/**"
}
```

### Multiple specs

You can track coverage against multiple specs:

```kdl
spec {
    name "core-spec"
    rules_glob "docs/core/**/*.md"
    include "src/core/**/*.rs"
}

spec {
    name "extension-spec"
    rules_glob "docs/extensions/**/*.md"
    include "src/extensions/**/*.rs"
}
```

## CLI Usage

### Coverage report

```bash
# Run coverage report
tracey

# Check mode (exit 1 if below threshold)
tracey --check --threshold 80

# Verbose output (shows all references)
tracey -v

# JSON output
tracey -f json

# Custom config file
tracey -c path/to/config.kdl
```

### Interactive dashboard

Launch a web-based dashboard for exploring coverage and traceability:

```bash
# Start dashboard server
tracey serve

# Open browser automatically
tracey serve --open

# Custom port
tracey serve --port 8080
```

The dashboard provides:
- Visual coverage overview
- Searchable rule list with coverage status
- Click-through to source code locations
- Live reload when spec or source files change

### Traceability matrix

Generate a matrix showing which code implements/verifies each rule:

```bash
# Markdown table
tracey matrix

# HTML report (opens in browser)
tracey matrix --format html --open

# Show only uncovered rules
tracey matrix --uncovered

# Show rules missing tests
tracey matrix --no-verify

# Filter by rule prefix
tracey matrix --prefix "channel."

# Filter by requirement level
tracey matrix --level must
```

### Reverse coverage

Show which code units (functions, structs, etc.) lack spec references:

```bash
# Full reverse coverage report
tracey rev

# Only show uncovered code units
tracey rev --uncovered

# Filter by code unit kind
tracey rev --kind function

# Filter by file path
tracey rev --path src/channel

# JSON output (for AI agents)
tracey rev --format json --uncovered
```

The JSON output is designed for AI agents to find and annotate uncovered code:

```json
{
  "summary": { "total": 100, "covered": 40, "uncovered": 60, "coverage_percent": 40.0 },
  "units": [
    { "kind": "function", "name": "process_request", "file": "src/handler.rs", "start_line": 42, "end_line": 67, "rule_refs": [], "covered": false }
  ]
}
```

### Impact analysis

Find all code that references a specific rule:

```bash
tracey impact channel.id.allocation
```

### Location query

Show which rules are referenced at a specific location:

```bash
# Single line
tracey at src/channel.rs:42

# Line range
tracey at src/channel.rs:40-60

# Whole file
tracey at src/channel.rs
```

### Extracting rules from markdown

Generate a `_rules.json` manifest from spec documents:

```bash
# Output to stdout
tracey rules docs/spec/**/*.md

# Output to file with base URL
tracey rules -b "/spec" -o _rules.json docs/spec/**/*.md

# Also generate transformed markdown with HTML anchors
tracey rules --markdown-out dist/ docs/spec/**/*.md
```

tracey also warns about potential spec quality issues:
- Rules without RFC 2119 keywords (MUST, SHOULD, MAY) may be underspecified
- Rules with MUST NOT/SHALL NOT are hard to verify - consider rephrasing as positive requirements

## Library Usage

tracey-core can be used as a library:

```rust
use tracey_core::{Rules, WalkSources, SpecManifest, CoverageReport};
use tracey_core::markdown::{MarkdownProcessor, RulesManifest};

// Extract rules from markdown
let markdown = std::fs::read_to_string("spec.md")?;
let processed = MarkdownProcessor::process(&markdown)?;
println!("Found {} rules", processed.rules.len());

// Generate manifest JSON
let manifest = RulesManifest::from_rules(&processed.rules, "/spec");
println!("{}", manifest.to_json());

// Or scan source files for rule references
let rules = Rules::extract(
    WalkSources::new(".")
        .include(["**/*.rs", "**/*.swift", "**/*.ts"])
        .exclude(["target/**", "node_modules/**"])
)?;

// Compute coverage
let spec = SpecManifest::load("_rules.json")?;
let report = CoverageReport::compute("my-spec", &spec, &rules);
println!("Coverage: {:.1}%", report.coverage_percent());
```

## JSON Manifest Format

The `_rules.json` format (compatible with [dodeca](https://github.com/bearcove/dodeca)):

```json
{
  "rules": {
    "channel.id.allocation": {
      "url": "/spec/#r-channel.id.allocation"
    },
    "channel.id.parity": {
      "url": "/spec/#r-channel.id.parity"
    }
  }
}
```

## License

MIT OR Apache-2.0
