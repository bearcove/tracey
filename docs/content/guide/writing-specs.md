+++
title = "Writing Specs"
+++

Specifications are markdown documents containing requirements. Each requirement has a unique ID and describes a single behavior or constraint that is both implementable and testable.

## Requirement markers

Define requirements using the syntax `PREFIX[requirement.id]`:

```markdown
r[auth.login]
The system must accept a username and password and return a session token.
```

The requirement marker must appear as its own paragraph — either at column 0 or inside a blockquote. Everything following the marker until the next blank line, heading, or another marker is the requirement text.

### Blockquote form

Use blockquotes when a requirement contains multiple paragraphs, code blocks, or other rich content:

```markdown
> r[api.error-format]
> API errors must follow this format:
>
> ```json
> {"error": "message", "code": 400}
> ```
```

### Inline markers are ignored

Markers that appear inline within text are treated as regular text, not requirement definitions:

```markdown
When implementing r[auth.login] you should...
```

This does **not** define a requirement. Only markers at column 0 or inside a blockquote count.

## Prefixes

The prefix (`r` in `r[auth.login]`) identifies which spec a requirement belongs to. Tracey infers prefixes from what you write in your spec files — you don't configure them. Any lowercase alphanumeric string works:

- `r` — common default
- `h2` — for an HTTP/2 spec
- `req` — more explicit
- `api` — for an API spec

Each spec's prefix is determined by the markers in its markdown files. If your spec uses `r[...]`, that spec's prefix is `r`.

## Naming requirements

Requirement IDs are dot-separated segments:

```
section.subsection.name
```

Each segment may contain ASCII letters, digits, hyphens, and underscores. IDs must contain at least one dot.

**Valid IDs:**
- `auth.login`
- `api.v2.response-format`
- `channel.id_allocation`
- `crypto.sha256.validation`

**Invalid IDs:**
- `login` — no dot (single segment)
- `auth..login` — empty segment
- `auth.login.` — trailing dot
- `.auth.login` — leading dot
- `auth.lo gin` — spaces not allowed

### Naming conventions

Use a consistent hierarchy that mirrors your spec's section structure:

```markdown
# Authentication

r[auth.login]
...

r[auth.token-expiry]
...

## Password Policy

r[auth.password.min-length]
...

r[auth.password.complexity]
...
```

## Structuring your spec

Use markdown headings to organize requirements into sections. The tracey dashboard groups coverage by heading, so a well-structured spec produces a useful coverage outline:

```markdown
# Channel Management

## ID Allocation

r[channel.id.sequential]
Channel IDs must be allocated sequentially starting from 0.

r[channel.id.parity]
Client-initiated channels must use odd IDs, server-initiated channels must use even IDs.

## Lifecycle

r[channel.lifecycle.open]
A channel must be explicitly opened before any data can be sent.

r[channel.lifecycle.close]
Either peer may close a channel at any time. The closing peer must send a close frame.
```

## Avoiding duplicates

**Same file:** The same requirement ID appearing twice in one file is an error.

**Same spec, different files:** The same requirement ID in two files belonging to the same spec is also an error. Requirement IDs must be unique within a spec.

**Different specs:** Different specs may reuse the same requirement ID freely, because they have different prefixes:

```markdown
<!-- docs/internal-spec/api.md — prefix "r" -->
r[api.format]
API responses must use JSON.

<!-- vendor/messaging-spec/format.md — prefix "m" -->
m[api.format]
Messages must use Protocol Buffers.
```

These don't conflict because `r[api.format]` and `m[api.format]` belong to different specs.

## Versioning

Requirements can carry a version suffix like `r[auth.login+2]`. This is covered in detail in [Versioning](versioning.md). The short version: when you change a requirement's text, you bump its version number so tracey can tell you which code references are stale.
