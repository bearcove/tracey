---
name: tracey
description: Add proper Tracey spec annotations to code, find requirements, and check coverage. Use when working with projects that have Tracey configuration (.config/tracey/config.styx), when adding spec references to code, or when checking requirement coverage — including projects whose requirements are StrictDoc (.sdoc) files referenced with @relation(...) markers.
---

# Tracey

Add proper spec annotations to code using Tracey's requirement tracking system.

## Overview

Tracey maintains traceability between specification requirements and code. Specs can be markdown, Typst, AsciiDoc, or [StrictDoc](https://strictdoc.readthedocs.io/) (`.sdoc`) files. This skill helps add proper `r[impl req.id]` and `r[verify req.id]` annotations to code — or StrictDoc `@relation(UID, …)` markers for `.sdoc` specs — find which requirements need implementation or testing, and understand Tracey annotation syntax.

**Primary Interface:** Use Tracey MCP tools (`tracey_status`, `tracey_uncovered`, `tracey_untested`, `tracey_stale`, `tracey_rule`, `tracey_unmapped`, `tracey_validate`, `tracey_config`) to discover requirements and validate coverage. The MCP tools provide self-documenting output showing which prefixes to use and what requirements need work.

## When to Use

Use this skill when:
- Working with a project that has `.config/tracey/config.styx`
- User asks to "add spec annotations" or "annotate with requirements"
- Need to find which requirements are uncovered or untested
- Writing code that implements spec requirements
- Writing tests that verify spec requirements

## Quick Reference: MCP Tools

| Tool | Purpose |
|------|---------|
| `tracey_status` | See configured specs, prefixes, and coverage percentages |
| `tracey_uncovered` | List requirements without implementation |
| `tracey_untested` | List requirements without verification/tests |
| `tracey_stale` | List references that point to older requirement versions |
| `tracey_unmapped` | Show code that lacks requirement references |
| `tracey_rule <id>` | Get full details about a specific requirement |
| `tracey_validate` | Validate references and naming for a spec/impl |
| `tracey_config` | Display configured specs, impls, include/exclude globs |

**Tip:** Start with `tracey_status` to see what prefix to use (e.g., `r[...]` vs `shm[...]`) — or whether the spec is StrictDoc, in which case see [StrictDoc specs](#strictdoc-sdoc-specs) — then use `tracey_uncovered` or `tracey_untested` to find work that needs doing.

## Workflow: Adding Annotations

### Step 1: Check Configuration and Prefixes

Use the `tracey_status` MCP tool to see configured specs and their prefixes:

```
tracey_status
```

This shows:
- Which specs are configured (e.g., `rapace`, `shm`, `rust`)
- What prefix to use for each (e.g., `r[...]`, `shm[...]`, `rs[...]`)
- Current coverage percentages by implementation

**Example output:**
```
## Configured Specs

- **rapace** (prefix: `r`)
  - When annotating code, use: `r[impl rule.id]` or `r[verify rule.id]`

- **shm** (prefix: `shm`)
  - When annotating code, use: `shm[impl rule.id]` or `shm[verify rule.id]`
```

### Step 2: Find Relevant Requirements

Use Tracey MCP tools to discover which requirements need work:

**Find requirements without implementation:**
```
tracey_uncovered --spec_impl "rapace/rust"
```

**Find requirements without tests:**
```
tracey_untested --spec_impl "rapace/typescript"
```

**Get details about a specific requirement:**
```
tracey_rule "message.hello.timing"
```

**See which code lacks requirement references:**
```
tracey_unmapped --path "src/auth/"
```

### Step 3: Add Annotations

Add comments above relevant code using `PREFIX[VERB REQ]` syntax:

```rust
// r[impl auth.token.validation]
fn validate_token(token: &str) -> bool {
    // implementation
}
```

For tests:

```rust
// r[verify auth.token.validation]
#[test]
fn test_token_validation() {
    assert!(validate_token("valid"));
}
```

### Step 4: Validate

Use `tracey_status` to verify your annotations were detected and coverage improved:

```
tracey_status
```

Check that the implementation/verification percentages increased for your target spec/impl combination.

## StrictDoc (`.sdoc`) specs

Tracey reads StrictDoc `.sdoc` files natively — no conversion step, no markdown copy of the spec. You are looking at a StrictDoc project when `tracey_status` says the requirements are in "StrictDoc .sdoc files", or the spec `include` globs in `.config/tracey/config.styx` match `*.sdoc`.

**Requirement IDs are the `UID:` fields, verbatim.** A node with `UID: BR-001` is the rule `BR-001` — uppercase, case-sensitive, not lowercased, no `spec.` namespace. Any node with a `UID:` counts: `[REQUIREMENT]`, custom-grammar elements like `[FEATURE]`, and composite `[[…]]` nodes and their children. `[TEXT]` and sections are not requirements. Look requirements up with `tracey_rule` using the UID (`tracey_rule "BR-001"`).

**Annotate with StrictDoc's own markers** so both StrictDoc and tracey see them:

```rust
// @relation(BR-001, scope=function)
fn connect() { ... }

// @relation(BR-001, BR-002, scope=function, role=Verifies)
#[test]
fn test_connect_and_heartbeat() { ... }
```

```python
# @relation(BR-003, scope=class)
class Reconnector: ...
```

- **Verb from `role=`:** no role (or `Implements` / `Implementation`) → impl; `Verifies` / `Verification` / `Test` → verify. Other roles such as `Refines` are not counted and produce a warning.
- **`scope=`:** `file`, `class`, `function`, `line`, or a `range_start` … `range_end` pair around a block. Tracey doesn't use it for matching, but StrictDoc does, so pick the one that describes the annotated code.
- **Syntax is strict, as in StrictDoc:** UIDs first, then `scope=`, then `role=`, separated by `, ` (comma *and* space). `@relation{…}` also works.
- **The marker must start a comment line** (after `//`, `#`, `*`, …). A mention in prose (`// see @relation(BR-001) for details`) is ignored.
- `r[impl BR-001]` / `r[verify BR-001]` also work against `.sdoc` specs (the prefix is always `r`), but prefer `@relation` in a StrictDoc project, and don't add an `r[...]` marker next to an existing `@relation` for the same UID — it's already counted.
- Malformed markers show up in `tracey_validate`; fix them rather than adding a second marker.

## Annotation Syntax

### Supported Verbs

- **`impl`** - This code implements the requirement (default if verb omitted)
- **`verify`** - This code tests/verifies the requirement
- **`depends`** - This code must be reviewed if the requirement changes
- **`related`** - Loosely connected, shown when reviewing related code

### Language-Specific Examples

**Rust:**
```rust
// r[impl database.connection]
fn connect_to_db() -> Result<Connection> { ... }

/// r[verify database.connection]
#[test]
fn test_connection() { ... }
```

**TypeScript:**
```typescript
// r[impl api.error.format]
function formatError(code: number, message: string) { ... }

// r[verify api.error.format]
test('error format', () => { ... });
```

**Python:**
```python
# r[impl auth.validation]
def validate_user(token: str) -> bool:
    pass

# r[verify auth.validation]
def test_validate_user():
    pass
```

**Swift:**
```swift
// r[impl session.timeout]
func cleanupExpiredSessions() { ... }

// r[verify session.timeout]
func testSessionTimeout() { ... }
```

## Common Patterns

### Multiple Requirements

One implementation can satisfy multiple requirements:

```rust
// r[impl auth.validation]
// r[impl auth.rate-limiting]
fn validate_with_rate_limit() { ... }
```

### Generated Code

Annotate generators, not generated code:

```rust
// r[impl codegen.service-dispatch]
fn generate_dispatcher(service: &ServiceDetail) -> String {
    // Code generator that produces dispatchers
}
```

### Partial Implementation

Use clear context when one requirement has multiple implementing functions:

```rust
// r[impl database.connection] - connection pooling
fn create_pool() { ... }

// r[impl database.connection] - connection lifecycle
fn close_connection() { ... }
```

## Configuration Example

**Note:** Instead of manually reading the config file, use `tracey_status` to see configured specs, prefixes, and implementations.

Typical `.config/tracey/config.styx`:

```styx
specs (
    {
        name my-project
        include (docs/spec/**/*.md)
        impls (
            {
                name rust
                include (
                    crates/**/*.rs
                )
                exclude (target/**)
            }
            {
                name typescript
                include (src/**/*.ts)
            }
        )
    }
)
```

Key fields:
- `include` (spec) - Glob patterns for spec files (`.md`, `.typ`, `.adoc`, `.sdoc`), as a sequence `(...)`. The annotation prefix (e.g. `r` in `r[...]`) is inferred from the spec's requirement markers; StrictDoc specs always use `r`.
- `impls` - Sequence of implementation blocks
- `include` (impl) - Glob patterns for source files to scan, as a sequence `(...)`
- `exclude` (impl) - Glob patterns to exclude, relative to the project root, as a sequence `(...)`. Also applies to `test_include` files.
- `test_include` (impl) - Glob patterns for test files, which may only contain `verify` references

## Troubleshooting

**Error: Unknown prefix**
- Check the prefix in `.config/tracey/config.styx`
- Ensure annotations use the correct prefix

**Requirements not found**
- Verify requirement IDs match exactly (case-sensitive). Markdown IDs are usually dot-separated (`auth.login`); StrictDoc IDs are the `UID:` values as written (`BR-001`)
- Check that spec files are in the configured `include` paths

**Annotations not detected**
- Ensure annotations are in comments (not strings)
- Check that source files match the `include` patterns in config
- For `@relation`: check that the marker starts the comment line, uses `, ` between arguments, and has UIDs before `scope=` and `role=`; `tracey_validate` reports malformed markers

## Reference Documentation

For detailed information about Tracey's implementation, configuration schema, and tooling, see the `references/tracey-spec.md` file bundled with this skill.
