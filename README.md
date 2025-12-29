# tracey

> **Note:** Looking for Tracy, the frame profiler? That's a different project: [wolfpld/tracy](https://github.com/wolfpld/tracy)

A CLI tool and library to measure spec coverage in codebases.

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
| `[rule.id]` | Basic reference (legacy) |
| `[impl rule.id]` | Implementation of the rule |
| `[verify rule.id]` | Test/verification of the rule |
| `[test rule.id]` | Alias for verify |
| `[ref rule.id]` | General reference |

The verb helps categorize references in the coverage report.

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

### Extracting rules from markdown

You can also use tracey to generate a `_rules.json` manifest:

```bash
# Output to stdout
tracey rules docs/spec/**/*.md

# Output to file with base URL
tracey rules -b "/spec" -o _rules.json docs/spec/**/*.md

# Also generate transformed markdown with HTML anchors
tracey rules --markdown-out dist/ docs/spec/**/*.md
```

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
