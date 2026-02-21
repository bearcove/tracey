# tracey

> **Note:** Looking for Tracy, the frame profiler? That's a different project: [wolfpld/tracy](https://github.com/wolfpld/tracy)

A CLI tool and library to measure spec coverage in codebases, with an interactive dashboard for exploring traceability.

## What it does

tracey maintains traceability between specifications and code. It uses lightweight annotations in markdown and source code comments to link specification requirements with implementing code and tests.

This enables:
- Verifying multiple implementations match the same spec
- Finding which requirements lack implementation or tests  
- Seeing which requirement justifies each piece of code
- Analyzing impact when requirements or code changes

For the full specification, see [docs/spec/tracey.md](docs/spec/tracey.md).

## Installation

```bash
# With cargo-binstall (fast, downloads pre-built binary)
cargo binstall tracey

# Or build from source
cargo install tracey
```

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

### 2. Reference requirements in your code

Reference spec requirements in comments:

```rust
/// Allocates the next channel ID for this peer.
/// r[impl channel.id.parity]
fn allocate_channel_id(&mut self) -> u32 { ... }
```

Supported verbs: `impl`, `verify`, `depends`, `related` (defaults to `impl` if omitted).

### 3. Configure tracey

Create `.config/tracey/config.styx`:

```styx
specs (
  {
    name my-spec
    include (docs/spec/**/*.md)
    impls (
      {
        name main
        include (src/**/*.rs)
      }
    )
  }
)
```

### 4. Launch the dashboard

```bash
tracey serve
```

## License

[MIT](LICENSE-MIT) OR [Apache-2.0](LICENSE-APACHE)
