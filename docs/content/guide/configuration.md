+++
title = "Configuration"
weight = 4
+++

Tracey is configured via `.config/tracey/config.styx` in your project root. The config file uses the [Styx](https://styx.bearcove.eu/) configuration language and defines which spec files to read and which source files to scan.

## Minimal config

```styx
@schema {id crate:tracey-config@1, cli tracey}

specs (
    {
        name my-project
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

## Styx format

Styx is a configuration language. The basics:

- **Key-value:** `name my-project`
- **Lists:** `include (pattern1 pattern2 pattern3)` — items separated by whitespace
- **Blocks:** `{ ... }` — group related fields
- **Comments:** `// this is a comment`
- **Sequences of blocks:** `specs ( { ... } { ... } )`

## Spec fields

Each entry in `specs (...)` defines a specification:

| Field | Required | Description |
|-------|----------|-------------|
| `name` | Yes | Display name for this spec |
| `include` | Yes | Glob patterns matching your spec's markdown files |
| `source_url` | No | Canonical URL (e.g., GitHub repo) — shown in dashboard for attribution |
| `impls` | Yes | List of implementation configurations |

The prefix (e.g., `r` in `r[auth.login]`) is inferred from the requirement markers in your markdown files. You don't configure it.

```styx
{
    name my-api
    source_url https://github.com/example/my-api
    include (docs/spec/**/*.md)
    impls ( ... )
}
```

## Implementation fields

Each entry in `impls (...)` defines a set of source files to scan:

| Field | Required | Description |
|-------|----------|-------------|
| `name` | Yes | Implementation name (e.g., "rust", "typescript", "main") |
| `include` | No | Glob patterns for source files to scan. Defaults to `**/*.rs` if omitted |
| `exclude` | No | Glob patterns for files to skip |
| `test_include` | No | Glob patterns for test-only files (may only contain `verify` annotations) |

```styx
{
    name rust
    include (src/**/*.rs lib/**/*.rs)
    exclude (target/** generated/**)
    test_include (tests/**/*.rs)
}
```

### Test files

Files matched by `test_include` are scanned for `verify` annotations only. Using `impl` in a test file is a hard error. This enforces a clean separation between implementation and verification:

```styx
impls (
    {
        name rust
        include (src/**/*.rs)
        test_include (tests/**/*.rs)
    }
)
```

In this setup, `src/auth.rs` may contain `r[impl auth.login]` but `tests/auth_test.rs` may only contain `r[verify auth.login]`.

### Common exclude patterns

```styx
exclude (
    target/**
    node_modules/**
    vendor/**
    dist/**
    **/*.generated.*
)
```

File walking respects `.gitignore` automatically, so you usually don't need to exclude things like `target/` or `node_modules/` if they're already gitignored.

## Multiple implementations

Track coverage separately for different languages or components:

```styx
specs (
    {
        name my-api
        include (docs/spec/**/*.md)
        impls (
            {
                name rust-backend
                include (backend/src/**/*.rs)
            }
            {
                name typescript-frontend
                include (frontend/src/**/*.ts frontend/src/**/*.tsx)
            }
        )
    }
)
```

Each implementation gets its own coverage percentage in the dashboard. This is useful when the same spec is implemented in multiple languages or when different parts of the codebase cover different aspects of the spec.

## Multiple specs

Your project might implement both its own spec and an external one (e.g., an RFC or protocol spec obtained via git submodule):

```styx
specs (
    {
        name myapp
        include (docs/spec/**/*.md)
        impls (
            {
                name rust
                include (src/**/*.rs)
            }
        )
    }
    {
        name http2
        source_url https://github.com/http2/spec
        include (vendor/http2-spec/**/*.md)
        impls (
            {
                name rust
                include (src/http2/**/*.rs)
            }
        )
    }
)
```

With this configuration:
- `r[impl auth.login]` references the `myapp` spec (prefix `r`, inferred from `docs/spec/*.md`)
- `h2[impl stream.priority]` references the `http2` spec (prefix `h2`, inferred from `vendor/http2-spec/*.md`)

Different specs can even share a prefix — tracey uses requirement ID matching to disambiguate.

## Cross-workspace paths

Include patterns can reference files outside the project root using relative paths:

```styx
impls (
    {
        name main
        include (
            crates/**/*.rs
            ../other-crate/src/**/*.rs
        )
        exclude (
            target/**
            ../other-crate/target/**
        )
    }
)
```

Paths are resolved relative to the project root (where tracey is invoked or where the config file lives). If a referenced path doesn't exist on disk, tracey continues with a warning.

## Optional config file

The config file is optional. Tracey starts with empty defaults when no config exists and watches for the file to be created. This means you can start the daemon or LSP before creating your config — it will pick up the config automatically when you create it.

## Real-world example

Tracey's own configuration:

```styx
specs (
    {
        name tracey
        source_url https://github.com/bearcove/tracey
        include (docs/spec/**/*.md)
        impls (
            {
                name main
                include (
                    crates/**/*.rs
                    crates/**/*.ts
                    crates/**/*.tsx
                    crates/**/*.css
                    tracey-zed/src/**/*.rs
                    ../marq/**/*.rs
                )
                exclude (
                    target/**
                    ../marq/target/**
                    crates/tracey/tests/fixtures/**
                )
            }
        )
    }
)
```
