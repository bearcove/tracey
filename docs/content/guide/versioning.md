+++
title = "Versioning"
+++

Specs change over time. When a requirement's text changes, the implementing code might need to change too. Tracey's versioning system tracks whether code has been reviewed against the latest version of each requirement.

## How versions work

Requirement IDs can carry a version suffix:

```markdown
r[auth.login+2]
Users must authenticate using OAuth 2.0 (changed from basic auth).
```

- `r[auth.login]` — implicitly version 1
- `r[auth.login+2]` — explicitly version 2
- `r[auth.login+1]` — same as `r[auth.login]` (version 1 is the default)

Versions are positive integers starting at 1. A requirement without a version suffix is version 1.

## When a requirement changes

Suppose your spec originally has:

```markdown
r[auth.login]
Users must authenticate with a username and password.
```

And your code references it:

```rust
// r[impl auth.login]
fn login(username: &str, password: &str) -> Result<Token> { ... }
```

Later, you update the requirement to use OAuth:

```markdown
r[auth.login+2]
Users must authenticate using OAuth 2.0.
```

Now the code reference `r[impl auth.login]` is **stale** — it points to version 1, but the spec is at version 2. Tracey reports this as a warning: the code was written against an older version of the requirement and needs review.

## Stale references

A stale reference means "this code was written against an older version of the requirement." Stale requirements are not counted as covered.

Tracey surfaces stale references in:

- **Dashboard** — stale requirements are flagged in the coverage view
- **LSP diagnostics** — warnings appear in your editor
- **MCP tools** — `tracey_stale` lists all stale references
- **Terminal** — `tracey query stale` shows stale references

The stale warning message says:

> Implementation must be changed to match updated rule text — and ONLY ONCE THAT'S DONE must the code annotation be bumped

This is deliberate: update the code first, then bump the annotation. The annotation bump is a signal that you've reviewed and updated the implementation.

## Resolving stale references

1. **Read the diff** — LSP hover shows a word-level diff of what changed. `tracey query rule auth.login` shows the full current text.

2. **Update your code** — modify the implementation to match the new requirement. In our example, change the login function from username/password to OAuth.

3. **Bump the annotation** — update the code reference to the current version:

```rust
// r[impl auth.login+2]
fn login(oauth_token: &str) -> Result<Session> { ... }
```

Now the reference matches the current spec version, and tracey counts it as covered again.

## Automating version bumps in specs

When you edit a requirement's text in the spec, you need to bump its version number. Tracey provides two commands to help:

### Pre-commit check

```bash
tracey pre-commit
```

Checks staged spec files for requirements whose text changed without a version bump. Fails with an error if any are found. Install this as a git pre-commit hook to catch forgotten bumps:

```bash
#!/bin/sh
# .git/hooks/pre-commit
tracey pre-commit
```

### Auto-bump

```bash
tracey bump
```

Automatically increments the version number of staged requirements whose text changed, then re-stages the modified files. Run this before committing when you've edited spec text:

```bash
# Edit your spec, then:
git add docs/spec/api.md
tracey bump              # auto-bumps versions, re-stages
git commit -m "Update auth requirements"
```

## Viewing diffs

**LSP hover** — hover over a stale or recently-bumped reference to see a word-level diff with ~~strikethrough~~ for removed words and **bold** for added words.

**Terminal** — `tracey query rule auth.login` shows the full requirement text and all references. `tracey query stale` lists all stale references across the codebase.

**Dashboard** — the validation view flags stale references with details.

## Version in code references

Code references can also include version numbers:

```rust
// r[impl auth.login+2]
```

This explicitly references version 2. If the spec later moves to version 3, this reference becomes stale. References without a version suffix (`r[impl auth.login]`) are implicitly version 1.

When resolving a stale reference, always update to the current spec version to make it clear which version you've reviewed against.
