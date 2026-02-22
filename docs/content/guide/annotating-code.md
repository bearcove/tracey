+++
title = "Annotating Code"
weight = 3
+++

Requirement references link your source code back to the specification. They're written as comments using the syntax `PREFIX[VERB requirement.id]`.

## Basic syntax

```rust
// r[impl auth.login]
fn login(username: &str, password: &str) -> Result<Token> {
    // ...
}
```

The verb defaults to `impl` when omitted, so these are equivalent:

```rust
// r[impl auth.login]
// r[auth.login]
```

You can add text after the closing bracket — it's ignored by the parser:

```rust
// r[impl auth.login]: handles credential validation
```

## Verbs

| Verb | Meaning | Use for |
|------|---------|---------|
| `impl` | Implements the requirement | Production code that fulfills the spec |
| `verify` | Tests/verifies the requirement | Test code that asserts correct behavior |
| `depends` | Strict dependency | Code that must be rechecked if the requirement changes |
| `related` | Loose connection | Related code shown during review |

If no verb is given, `impl` is assumed.

### impl

Marks code that implements a requirement:

```rust
// r[impl channel.lifecycle.open]
fn open_channel(&mut self, id: u32) -> Result<()> {
    // ...
}
```

### verify

Marks test code that verifies a requirement:

```typescript
// r[verify channel.lifecycle.open]
test('channel must be opened before sending data', () => {
    const channel = new Channel();
    expect(() => channel.send(data)).toThrow('not open');
});
```

### depends

Marks code with a strict dependency on a requirement — it must be reviewed whenever that requirement changes:

```python
# r[depends auth.crypto.algorithm]
def hash_password(password: str) -> str:
    return bcrypt.hashpw(password.encode(), bcrypt.gensalt())
```

### related

Marks a loose connection, surfaced when reviewing related code:

```swift
// r[related user.session.timeout]
func cleanupExpiredSessions() {
    sessions.removeAll { $0.isExpired }
}
```

## Language examples

Tracey extracts annotations from comments in all major languages via tree-sitter:

**Rust** — line, doc, and block comments:
```rust
// r[impl auth.login]
/// r[impl auth.login]
//! r[impl auth.login]
/* r[impl auth.login] */
```

**TypeScript / JavaScript:**
```typescript
// r[impl api.response-format]
/* r[verify api.response-format] */
```

**Python:**
```python
# r[impl auth.validation]
```

**Go:**
```go
// r[impl stream.priority]
/* r[verify stream.priority] */
```

**Swift:**
```swift
// r[impl session.timeout]
```

**Java:**
```java
// r[impl database.connection]
/** r[verify database.connection] */
```

**C / C++:**
```c
// r[impl buffer.allocation]
/* r[verify buffer.allocation] */
```

## Multiple annotations per function

A single function can implement multiple requirements:

```rust
// r[impl auth.validation]
// r[impl auth.rate-limiting]
fn validate_with_rate_limit(credentials: &Credentials) -> Result<()> {
    check_rate_limit(credentials.ip)?;
    verify_credentials(credentials)?;
    Ok(())
}
```

## Multiple functions per requirement

A single requirement can be implemented across multiple functions. Adding a trailing comment can help clarify:

```rust
// r[impl database.connection]
fn create_pool(config: &DbConfig) -> Pool {
    // connection pooling
}

// r[impl database.connection]
fn close_connection(conn: Connection) {
    // connection lifecycle
}
```

## Test files

If your config uses the `test_include` field to designate test files, those files may only contain `verify` annotations. Using `impl` in a test file is an error. See [Configuration](configuration.md) for details.

## Ignore directives

Sometimes source code mentions requirement syntax in documentation, test fixtures, or string literals where it shouldn't be extracted. There are several ways to suppress extraction.

### Backticks

Annotations inside backticks in comments are ignored:

```rust
// This is `r[impl not.an.annotation]`, just a comment
```

### Ignore next line

```rust
// @tracey:ignore-next-line
// This mentions r[impl auth.login] but it won't be extracted
```

### Ignore block

```rust
// @tracey:ignore-start
// These fixtures contain r[impl auth.login] and r[impl api.format]
// but they're just test data, not actual annotations
// @tracey:ignore-end
```

Ignore blocks must not nest. An unclosed `@tracey:ignore-start` is reported as an error during validation.

## Multiple specs

When your project implements multiple specs (each with a different prefix), use the appropriate prefix for each:

```rust
// r[impl auth.login]          // internal spec (prefix "r")
// h2[impl stream.priority]    // HTTP/2 spec (prefix "h2")
```

The prefix routes the annotation to the correct spec. See [Configuration](configuration.md) for setting up multiple specs.
