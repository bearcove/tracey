+++
title = "Tracey"
description = "Spec coverage tooling"
+++

<div class="feature-row">
<div class="feature-text">

## Write specs in Markdown

Define requirements with `r[rule.id]` markers in any Markdown file. Tracey parses them into a structured rule tree with parent-child relationships.

</div>
<div class="feature-visual">

```markdown
## Connection Lifecycle

r[conn]

r[conn.open]
The client MUST send a handshake frame
before any other communication.

r[conn.close]
Either side MAY initiate a graceful close.
```

</div>
</div>

<div class="feature-row">
<div class="feature-text">

## Annotate your code

Reference rules with `// [impl conn.open]` comments in your source. Tracey scans Rust, TypeScript, Python, Go, Java, and Swift.

</div>
<div class="feature-visual">

```rust
// [impl conn.open]
fn open_connection(&mut self) -> Result<()> {
    self.send_handshake()?;
    self.state = State::Open;
    Ok(())
}

#[test]
fn test_handshake() {
    // [verify conn.open]
    let mut conn = Connection::new();
    assert!(conn.open_connection().is_ok());
}
```

</div>
</div>

<div class="feature-row">
<div class="feature-text">

## See what's covered

The CLI and web dashboard show coverage at a glance — which rules have implementations, which have tests, and which have gone stale after a spec change.

</div>
<div class="feature-visual">

```text
$ tracey status

  Spec    Coverage  Tested  Stale
  roam    87.3%     64.1%   2 rules
  styx    91.0%     78.4%   0 rules

$ tracey uncovered

  r[conn.close.timeout]  No implementations found
  r[enc.aead.rekey]      No implementations found
```

</div>
</div>

<div class="feature-row">
<div class="feature-text">

## Integrates everywhere

MCP server for AI-assisted editors (Zed, VS Code). Web dashboard with `tracey serve`. CI-friendly exit codes. Spec versioning catches drift automatically.

</div>
<div class="feature-visual">

```shell
# In your editor — MCP integration
tracey mcp

# Web dashboard on localhost
tracey serve

# CI: fail if coverage drops
tracey validate --min-coverage 80
```

</div>
</div>
