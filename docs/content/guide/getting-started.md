+++
title = "Getting Started"
weight = 1
+++

This guide takes you from zero to a working tracey setup: a spec with requirements, annotated source code, and a coverage dashboard.

## Install tracey

```bash
# Pre-built binary (fast, from GitHub Releases)
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/bearcove/tracey/releases/latest/download/tracey-installer.sh | sh

# Or build from source (main branch)
cargo install --locked --git https://github.com/bearcove/tracey --branch main tracey
```

## Create your spec

Create a markdown file with your requirements. Each requirement uses the syntax `r[requirement.id]` followed by its text:

```markdown
<!-- docs/spec/api.md -->

# API Specification

## Authentication

r[auth.login]
The system must accept a username and password and return a session token on success.

r[auth.token-expiry]
Session tokens must expire after 24 hours of inactivity.
```

The `r` prefix is just a convention — you can use any lowercase alphanumeric prefix like `req`, `h2`, or `api`. Tracey infers the prefix from what you write.

## Annotate your code

Reference requirements from source code comments using `PREFIX[VERB req.id]`:

```rust
// src/auth.rs

// r[impl auth.login]
pub fn login(username: &str, password: &str) -> Result<Token> {
    let user = verify_credentials(username, password)?;
    Ok(Token::new(user.id))
}

// r[impl auth.token-expiry]
pub fn is_token_valid(token: &Token) -> bool {
    token.age() < Duration::hours(24)
}
```

The verb defaults to `impl` if omitted, so `r[auth.login]` and `r[impl auth.login]` are equivalent. Other verbs: `verify` for tests, `depends` for strict dependencies, `related` for loose connections.

```rust
// tests/auth_test.rs

// r[verify auth.login]
#[test]
fn test_login_success() {
    let token = login("alice", "correct-password").unwrap();
    assert!(is_token_valid(&token));
}
```

## Configure tracey

Create `.config/tracey/config.styx` in your project root:

```styx
specs (
    {
        name my-api
        include (docs/spec/**/*.md)
        impls (
            {
                name rust
                include (src/**/*.rs)
            }
        )
    }
)
```

This tells tracey where to find your spec files and which source files to scan for annotations.

## Launch the dashboard

```bash
tracey web --open
```

This starts the dashboard at `http://localhost:3000` and opens it in your browser. You'll see your spec rendered with coverage badges showing which requirements have implementations.

## Check from the terminal

You can also query coverage without the dashboard:

```bash
# Coverage overview
tracey query status

# List requirements without implementations
tracey query uncovered

# List requirements without tests
tracey query untested
```

## What's next

- [Writing Specs](writing-specs.md) — requirement syntax, naming conventions, and document structure
- [Annotating Code](annotating-code.md) — all verbs, language examples, and ignore directives
- [Configuration](configuration.md) — multiple specs, test files, exclude patterns
- [Dashboard](dashboard.md) — navigating the web UI
- [Editor Integration](editor-integration.md) — LSP setup for real-time feedback
- [Versioning](versioning.md) — tracking spec changes over time
