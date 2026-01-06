# Tracey Specification

## Introduction

Tracey maintains traceability between specifications and code. Specs, implementations, and tests drift apart—code changes without updating specs, specs describe unimplemented features, tests cover different scenarios than requirements specify.

Tracey uses lightweight annotations in markdown and source code comments to link specification requirements with implementing code, tests, and dependencies. This enables:

- Verifying multiple implementations (different languages, platforms) match the same spec
- Finding which requirements lack implementation or tests
- Seeing which requirement justifies each piece of code
- Analyzing impact when requirements or code changes

This document has two parts: **Language** (annotation syntax) and **Tooling** (tracey implementation).

## Nomenclature

To avoid confusion, we define these terms precisely and use them consistently:

**Specification** (or **spec**)
A set of requirements, typically written as a human-readable document.

**Requirement** (or **req**)
A single, positive (MUST, not MUST NOT) property of the system that is both implementable and testable. Each requirement should describe one specific behavior or constraint.

**Implementation**
Code that fulfills a requirement's behavior or constraint.

**Test**
Code that verifies an implementation correctly fulfills a requirement, typically containing assertions run by a test harness.

**Important distinctions:**
- A **spec** is a document containing requirements. **Tests** are executable code. Don't use "spec" to mean "test file" (as some test frameworks do).
- Other projects may use "rule" to mean requirement. We don't use that term—use "requirement" or "req" instead.

---

# Language

This section specifies the annotation language: how to define requirements in markdown specifications and reference them in source code.

## Requirement Definitions in Markdown

Requirements are defined in markdown specification documents using the syntax `PREFIX[REQ]` where PREFIX is the spec's configured prefix and REQ is a requirement ID.

### Markdown Requirement Syntax

> r[markdown.syntax.marker]
> A requirement definition MUST be written as `PREFIX[REQ]` in one of two contexts: as a standalone paragraph starting at column 0, or inside a blockquote. The PREFIX identifies which spec this requirement belongs to (configured via `prefix` in the spec configuration). The VERB is implicitly "define" in markdown (unlike source code which uses explicit verbs like `r[impl REQ]`).
>
> Valid (standalone):
> ```markdown
> r[auth.token.validation]
> The system must validate tokens before granting access.
> ```
>
> Valid (in blockquote for multi-paragraph content):
> ```markdown
> > r[api.error.format]
> > API errors must follow this format:
> >
> > ```json
> > {"error": "message", "code": 400}
> > ```
> ```

> r[markdown.syntax.inline-ignored]
> Requirement markers that appear inline within other text MUST be treated as regular text, not requirement definitions.
>
> Valid (defines requirement):
> ```markdown
> r[database.connection]
> When connecting to the database...
> ```
>
> Invalid (treated as text, not a definition):
> ```markdown
> When implementing r[database.connection] you should...
> ```

### Duplicate Detection

> r[markdown.duplicates.same-file]
> If the same requirement ID appears multiple times in a single markdown file, an error MUST be reported.
>
> Invalid:
> ```markdown
> r[auth.validation]
> Users must authenticate.
>
> Later in the same file...
>
> r[auth.validation]
> This causes an error - duplicate requirement ID!
> ```

> r[markdown.duplicates.cross-file]
> If the same requirement ID appears in multiple markdown files within the same spec, an error MUST be reported when merging manifests. Requirement IDs only need to be unique within a single spec; different specs may use the same requirement ID since they have different prefixes.
>
> Invalid (same spec):
> ```markdown
> # docs/spec/auth.md (tracey spec, prefix "r")
> r[api.format]
> API responses must use JSON.
>
> # docs/spec/api.md (tracey spec, prefix "r")
> r[api.format]
> Error - this requirement ID is already defined in auth.md within the same spec!
> ```
>
> Valid (different specs):
> ```markdown
> # docs/tracey/spec.md (tracey spec, prefix "r")
> r[api.format]
> API responses must use JSON.
>
> # vendor/messaging-spec/spec.md (messaging spec, prefix "m")
> m[api.format]
> OK - different spec, different prefix, no conflict.
> ```

## Requirement References in Source Code

Requirement references are extracted from source code comments using the syntax `PREFIX[VERB REQ]` where PREFIX matches a configured spec's prefix.

### Basic Syntax

r[ref.syntax.brackets]
A requirement reference MUST be written as `PREFIX[VERB REQ]` within a comment, where PREFIX identifies which spec is being referenced (matching the `prefix` field in the spec configuration).

> r[ref.syntax.verb]
> VERB indicates the relationship type (impl, verify, depends, related).
> 
> If omitted, defaults to `impl`.

> r[ref.syntax.req-id]
> REQ is a requirement ID consisting of dot-separated segments.
>
> Each segment MUST contain only ASCII letters (a-z, A-Z), digits (0-9), hyphens, or underscores. This restriction ensures requirement IDs work cleanly in URLs without encoding issues.

> r[ref.syntax.surrounding-text]
> The annotation MAY be surrounded by other text within the comment. Any characters (including punctuation) after the closing `]` are ignored by the parser.
>
> Valid:
> ```rust
> // r[impl auth.token.validation]       // tracey spec (prefix "r")
> // r[verify user-profile.update_email] // tracey spec
> // m[depends crypto_v2.algorithm]      // messaging spec (prefix "m")
> // h2[impl api.v1.users]               // http2 spec (prefix "h2")
> // r[message.hello.timing]: send Hello immediately
> // See r[auth.requirements] for details.
> ```
>
> Invalid:
> ```rust
> // r[impl auth..token]           // empty segment
> // r[verify user.profile!update] // exclamation mark not allowed
> // r[depends .crypto.algorithm]  // leading dot
> // r[impl api.users.]            // trailing dot
> // r[verify user profile.update] // space not allowed
> // r[impl auth.🔐.token]         // emoji not allowed
> // r[verify café.menu]           // accented characters not allowed
> ```

### Supported Verbs

Source code references use verbs to indicate the relationship between code and requirements:

> r[ref.verb.impl]
> The `impl` verb MUST be interpreted as indicating that the code implements the referenced requirement.
>
> ```rust
> // r[impl auth.token.validation]
> fn validate_token(token: &str) -> bool {
>     // etc.
> }
> ```

> r[ref.verb.verify]
> The `verify` verb MUST be interpreted as indicating that the code tests or verifies the referenced requirement.
>
> ```typescript
> test('token validation', () => {
>     // r[verify auth.token.validation]
>     expect(validateToken('abc')).toBe(true);
> });
> ```

> r[ref.verb.depends]
> The `depends` verb MUST be interpreted as indicating a strict dependency — the code must be rechecked if the referenced requirement changes.
>
> ```python
> # r[depends auth.crypto.algorithm]
> # This code must be reviewed if the crypto algorithm changes
> def hash_password(password: str) -> str:
>     return bcrypt.hashpw(password.encode(), bcrypt.gensalt())
> ```

> r[ref.verb.related]
> The `related` verb MUST be interpreted as indicating a loose connection, shown when reviewing related code.
>
> ```swift
> // r[related user.session.timeout]
> // Session cleanup is related to timeout requirements
> func cleanupExpiredSessions() {
>     sessions.removeAll { $0.isExpired }
> }
> ```

> r[ref.verb.default]
> When no verb is provided, the reference SHOULD be treated as an `impl` reference.
>
> ```go
> // r[auth.token.validation] - no verb, defaults to 'impl'
> func ValidateToken(token string) bool {
>     return len(token) > 0
> }
> ```

### Comment Types

r[ref.comments.line]
Requirement references MUST be recognized in line comments (`//`, `#`, etc. depending on language).

r[ref.comments.block]
Requirement references MUST be recognized in block comments (`/* */`, `""" """`, etc. depending on language).

r[ref.comments.doc]
Requirement references MUST be recognized in documentation comments (`///`, `//!`, `/** */`, etc. depending on language).

### Source Location Tracking

r[ref.span.offset]
Each extracted requirement reference MUST include the byte offset of its location in the source file.

r[ref.span.length]
Each extracted requirement reference MUST include the byte length of the reference.

r[ref.span.file]
Each extracted requirement reference MUST include the path to the source file.

---

# Tooling

This section specifies how the tracey tool processes annotations, computes coverage, and exposes results.

## Coverage Computation

r[coverage.compute.percentage]
Coverage percentage MUST be calculated as (covered requirements / total requirements) * 100.

r[coverage.compute.covered]
Tracey MUST consider a requirement covered if at least one reference to it exists in the scanned source files.

r[coverage.compute.uncovered]
Requirements in the manifest with no references MUST be reported as uncovered.

r[coverage.compute.invalid]
References to requirement IDs not present in the manifest MUST be reported as invalid.

## Reference Extraction

r[ref.verb.unknown]
When an unrecognized verb is encountered, tracey MUST emit a warning but SHOULD still extract the requirement reference.

r[ref.prefix.unknown]
When a reference uses a prefix that does not match any configured spec, tracey MUST report an error indicating the unknown prefix and list the available spec prefixes.

r[ref.prefix.matching]
When extracting references from source code, tracey MUST match the prefix against configured specs to determine which spec's requirement namespace to query.

## Cross-Workspace Implementation References

r[ref.cross-workspace.paths]
Implementation references MAY be located in different crates or workspaces outside the primary project structure.

> r[ref.cross-workspace.path-resolution]
> When resolving file paths for implementation references, tracey MUST resolve relative paths from the workspace root (the directory where tracey is invoked or where the configuration file is located).
>
> Example: If bearmark is at `../bearmark` relative to the tracey workspace, implementation files in bearmark would be referenced as `../bearmark/src/reqs.rs`.

> r[ref.cross-workspace.missing-paths]
> When an implementation file path does not exist on the filesystem, tracey MUST continue functioning but MUST emit warnings indicating the missing path.

> r[ref.cross-workspace.cli-warnings]
> Missing implementation file paths MUST be reported in CLI output when running commands like `tracey status`, `tracey_uncovered`, or other tools that access implementation data.
>
> Example warning format:
> ```
> Warning: Implementation file not found: ../bearmark/src/reqs.rs
>   Referenced by spec 'tracey' implementation 'rust'
> ```

> r[ref.cross-workspace.dashboard-warnings]
> Missing implementation file paths MUST be displayed as warnings in the dashboard UI, with visual indicators (warning badges, error states) on affected requirements or source file listings.

> r[ref.cross-workspace.graceful-degradation]
> When implementation files are missing, tracey MUST still display available data (requirement definitions, other implementations) and MUST NOT crash or fail to start.

## Code Unit Extraction

Code units are semantic units of code (functions, structs, enums, traits, impl blocks, modules, etc.) that are used for reverse traceability—tracking what percentage of code is linked to specification requirements.

r[code-unit.definition]
A code unit MUST be identified by its kind (function, struct, enum, trait, impl, module, const, static, type alias, macro), optional name, file path, start line, end line, and associated requirement references.

r[code-unit.boundary.include-comments]
When a code unit has associated comments (preceding line comments, block comments, or attributes), the code unit's `start_line` MUST include those comments. Comments are considered "associated" with a code unit if they immediately precede it with no intervening non-comment, non-attribute nodes.

This ensures that when a user clicks on a requirement reference badge pointing to a comment line, the highlighted range correctly encompasses the entire code unit including its documentation.

r[code-unit.nested.smallest]
When multiple code units contain a given line (e.g., a function inside a module), the annotation MUST be associated with the smallest (most specific) code unit for coverage computation. For example, if `mod tests {}` spans lines 100-500 and `fn test_foo()` spans lines 120-130, a reference on line 125 MUST be counted as covering `fn test_foo()`, not `mod tests {}`.

r[code-unit.refs.extraction]
Requirement references in comments associated with a code unit MUST be extracted and stored with that code unit for coverage computation.

## Markdown Processing

### HTML Output

> r[markdown.html.div]
> When transforming markdown, each requirement marker MUST be replaced with a `<div>` element with class `requirement`.
>
> Input:
> ```markdown
> r[auth.token.validation]
> ```
>
> Output:
> ```html
> <div class="requirement" id="r-auth.token.validation">
>   <a href="#r-auth.token.validation">auth.<wbr>token.<wbr>validation</a>
> </div>
> ```

> r[markdown.html.anchor]
> The generated div MUST have an `id` attribute in the format `r-{req.id}` for linking.
>
> ```html
> <div class="requirement" id="r-api.response.format">
> ```

> r[markdown.html.link]
> The generated div MUST contain a link (`<a>`) pointing to its own anchor.
>
> ```html
> <a href="#r-user.login.flow">user.<wbr>login.<wbr>flow</a>
> ```

> r[markdown.html.wbr]
> Dots in the displayed requirement ID SHOULD be followed by `<wbr>` elements to allow line breaking.
>
> ```html
> database.<wbr>connection.<wbr>pool
> ```

## Configuration

r[config.format.kdl]
The configuration file MUST be in KDL format.

r[config.path.default]
The default configuration path MUST be `.config/tracey/config.kdl` relative to the project root.

> r[config.schema]
> The configuration MUST follow this schema:
>
> ```kdl
> spec {
>     name "tracey"
>     prefix "r"
>     include "docs/spec/**/*.md"
>
>     impl {
>         name "rust"
>         include "crates/**/*.rs"
>         exclude "target/**"
>     }
> }
>
> spec {
>     name "messaging-protocol"
>     prefix "m"
>     include "vendor/messaging-spec/**/*.md"
>     source_url "https://github.com/example/messaging-spec"
>
>     impl {
>         name "rust"
>         include "crates/**/*.rs"
>     }
> }
> ```

r[config.spec.name]
Each spec configuration MUST have a `name` child node with the spec name as its argument.

r[config.spec.prefix]
Each spec configuration MUST have a `prefix` child node specifying the single-character or multi-character prefix used to identify this spec in markdown and source code annotations.

r[config.spec.include]
Each spec configuration MUST have one or more `include` child nodes specifying glob patterns for markdown files containing requirement definitions.

r[config.spec.source-url]
Each spec configuration MAY have a `source_url` child node providing the canonical URL for the specification (e.g., a GitHub repository). This URL is used for attribution in the dashboard and documentation.

r[config.impl.name]
Each impl configuration MUST have a `name` child node identifying the implementation (e.g., "main", "core").

r[config.impl.include]
Each impl configuration MAY have one or more `include` child nodes specifying glob patterns for source files to scan.

r[config.impl.exclude]
Each impl configuration MAY have one or more `exclude` child nodes specifying glob patterns for source files to exclude.

### Multiple Specs and Remote Specs

r[config.multi-spec.prefix-namespace]
When multiple specs are configured, the prefix serves as the namespace to disambiguate which spec a requirement belongs to.

r[config.multi-spec.unique-within-spec]
Requirement IDs MUST be unique within a single spec, but MAY be duplicated across different specs (since they use different prefixes).

r[config.remote-spec.local-files]
Remote specifications MUST be obtained as local files before tracey can process them. Users can use git submodules, manual downloads, or any other method to obtain spec files locally.

> r[config.remote-spec.workflow]
> The recommended workflow for implementing a remote specification is:
>
> 1. Obtain the spec files locally (e.g., via git submodule or download)
> 2. Configure the spec with `include` pointing to the local files
> 3. Set `source_url` to the canonical spec location for attribution
> 4. Use the spec's configured prefix in source code annotations
>
> Example:
> ```kdl
> spec {
>     name "http2"
>     prefix "h2"
>     include "vendor/http2-spec/docs/**/*.md"
>     source_url "https://github.com/http2/spec"
>
>     impl {
>         name "rust"
>         include "crates/http2/**/*.rs"
>     }
> }
> ```

## File Walking

r[walk.gitignore]
File walking MUST respect `.gitignore` files.

r[walk.default-include]
When no include patterns are specified, tracey MUST default to `**/*.rs`.

## Dashboard

Tracey provides a web-based dashboard for browsing specifications, viewing coverage, and navigating source code.

### URL Scheme

r[dashboard.url.structure]
Dashboard URLs MUST follow the structure `/{specName}/{impl}/{view}` where `{specName}` is the name of a configured spec and `{impl}` is an implementation name.

r[dashboard.url.spec-view]
The specification view MUST be accessible at `/{specName}/{impl}/spec` with optional heading hash fragment `/{specName}/{impl}/spec#{headingSlug}`.

r[dashboard.url.coverage-view]
The coverage view MUST be accessible at `/{specName}/{impl}/coverage` with optional query parameters `?filter=impl|verify` and `?level=must|should|may`.

r[dashboard.url.sources-view]
The sources view MUST be accessible at `/{specName}/{impl}/sources` with optional file and line parameters `/{specName}/{impl}/sources/{filePath}:{lineNumber}`.

r[dashboard.url.context]
Source URLs MAY include a `?context={reqId}` query parameter to show requirement context in the sidebar.

r[dashboard.url.root-redirect]
Navigating to `/` MUST redirect to `/{defaultSpec}/{defaultImpl}/spec` where `{defaultSpec}` is the first configured spec and `{defaultImpl}` is its first implementation.

r[dashboard.url.invalid-spec]
Navigating to an invalid spec name SHOULD redirect to the first valid spec or display an error.

### API Endpoints

r[dashboard.api.config]
The `/api/config` endpoint MUST return the project configuration including `projectRoot` and `specs` array.

r[dashboard.api.spec]
The `/api/spec?spec={specName}&impl={impl}` endpoint MUST return the rendered HTML and outline for the named spec and implementation.

r[dashboard.api.forward]
The `/api/forward?spec={specName}&impl={impl}` endpoint MUST return the forward mapping (requirements to file references) for the specified implementation.

r[dashboard.api.reverse]
The `/api/reverse?spec={specName}&impl={impl}` endpoint MUST return the reverse mapping (files to requirement references) with coverage statistics for the specified implementation.

r[dashboard.api.file]
The `/api/file?spec={specName}&impl={impl}&path={filePath}` endpoint MUST return the file content, syntax-highlighted HTML, and code unit annotations.

r[dashboard.api.version]
The `/api/version` endpoint MUST return a version string that changes when any source data changes.

r[dashboard.api.websocket]
The server MUST provide a WebSocket endpoint that notifies connected clients when source data changes, sending the new version identifier.

r[dashboard.api.websocket-reconnect]
The dashboard MUST automatically reconnect to the WebSocket if the connection is lost, with exponential backoff.

### Link Generation

r[dashboard.links.spec-aware]
All links generated in rendered markdown MUST include the spec name and implementation as the first two path segments.

r[dashboard.links.req-links]
Requirement ID badges MUST link to `/{specName}/{impl}/spec?req={reqId}` to navigate to the requirement in the specification.

r[dashboard.links.impl-refs]
Implementation reference badges MUST link to `/{specName}/{impl}/sources/{filePath}:{line}?context={reqId}`.

r[dashboard.links.verify-refs]
Verification/test reference badges MUST link to `/{specName}/{impl}/sources/{filePath}:{line}?context={reqId}`.

r[dashboard.links.heading-links]
Heading links in the outline MUST link to `/{specName}/{impl}/spec#{headingSlug}`.

### Specification View

r[dashboard.spec.outline]
The specification view MUST display a collapsible outline tree of headings in a sidebar.

r[dashboard.spec.outline-coverage]
Each outline heading SHOULD display a coverage indicator showing the ratio of covered requirements within that section.

r[dashboard.spec.content]
The specification view MUST display the rendered markdown content with requirement containers.

r[dashboard.spec.req-highlight]
When navigating to a requirement via URL parameter `?req={reqId}`, the requirement container MUST be highlighted and scrolled into view.

r[dashboard.spec.heading-scroll]
When navigating to a heading via URL path, the heading MUST be scrolled into view.

r[dashboard.spec.switcher]
The header MUST always display spec and implementation switcher dropdowns, even when only one option is available.

r[dashboard.spec.switcher-single]
When only one spec or implementation is configured, the switcher MUST still be visible (showing the single option).

### Coverage View

r[dashboard.coverage.table]
The coverage view MUST display a table of all requirements with their coverage status.

r[dashboard.coverage.filter-type]
The coverage view MUST support filtering by reference type (impl, verify, or all).

r[dashboard.coverage.filter-level]
The coverage view MUST support filtering by RFC 2119 level (MUST, SHOULD, MAY, or all).

r[dashboard.coverage.stats]
The coverage view MUST display summary statistics including total requirements, covered count, and coverage percentage.

r[dashboard.coverage.req-links]
Each requirement in the coverage table MUST link to the requirement in the specification view.

r[dashboard.coverage.ref-links]
Each reference in the coverage table MUST link to the source location.

### Sources View

r[dashboard.sources.file-tree]
The sources view MUST display a collapsible file tree in a sidebar.

r[dashboard.sources.tree-coverage]
Each folder and file in the tree SHOULD display a coverage percentage badge.

r[dashboard.sources.code-view]
When a file is selected, the sources view MUST display the syntax-highlighted source code.

r[dashboard.sources.line-numbers]
The code view MUST display line numbers.

r[dashboard.sources.line-annotations]
Lines containing requirement references MUST be annotated with indicators showing which requirements are referenced.

r[dashboard.sources.line-highlight]
When navigating to a specific line, that line MUST be highlighted and scrolled into view.

r[dashboard.sources.req-context]
When a `?context={reqId}` parameter is present, the sidebar MUST display the requirement details and all its references.

r[dashboard.sources.editor-open]
Clicking a line number SHOULD open the file at that line in the configured editor.

### Search

r[dashboard.search.modal]
The search modal MUST be openable via keyboard shortcut (Cmd+K on Mac, Ctrl+K elsewhere).

r[dashboard.search.reqs]
Search MUST support finding requirements by ID or text content.

r[dashboard.search.files]
Search MUST support finding files by path.

r[dashboard.search.navigation]
Selecting a search result MUST navigate to the appropriate view (spec for requirements, sources for files).

r[dashboard.search.prioritize-spec]
Search results MUST prioritize requirements from the specification over source code matches, displaying spec requirements before source file results in the results list.

r[dashboard.search.render-requirements]
Requirement search results MUST be rendered as styled HTML using the same markdown renderer as the specification view, not as plain text, preserving formatting and visual hierarchy.

r[dashboard.search.requirement-styling]
Rendered requirement search results MUST include proper styling for requirement IDs, nested requirements, code blocks, and other markdown elements to maintain visual consistency with the specification view.

### Header

r[dashboard.header.nav-tabs]
The header MUST display navigation tabs for Specification, Coverage, and Sources views.

r[dashboard.header.nav-active]
The active view tab MUST be visually distinguished.

r[dashboard.header.nav-preserve-spec]
Navigation tabs MUST preserve the current spec name and language when switching views.

r[dashboard.header.search]
The header MUST display a search input that opens the search modal when clicked or focused.

r[dashboard.header.logo]
The header MUST display a "tracey" link to the project repository.

## Command Line Interface

Tracey provides a minimal command-line interface focused on serving.

### Commands

r[cli.no-args]
When invoked with no subcommand, tracey MUST display help text listing available commands.

r[cli.serve]
The `tracey serve` command MUST start the HTTP dashboard server.

r[cli.mcp]
The `tracey mcp` command MUST start an MCP (Model Context Protocol) server over stdio.

## Server Architecture

Both `tracey serve` (HTTP) and `tracey mcp` (MCP) share a common headless server core.

### File Watching

> r[server.watch.patterns-from-config]
> The file watcher MUST derive which files to watch from the configuration's `include` patterns (both spec includes and impl includes), rather than hardcoding watched directories. For example, if the config contains:
>
> ```kdl
> include "crates/**/*.rs"
> include "crates/tracey/dashboard/src/**/*.tsx"
> include "docs/spec/**/*.md"
> ```
>
> Then changes to `crates/foo/bar.rs`, `crates/tracey/dashboard/src/main.tsx`, and `docs/spec/tracey.md` should all trigger rebuilds.

r[server.watch.respect-gitignore]
The file watcher MUST respect `.gitignore` rules, not triggering rebuilds for ignored files even if they match include patterns.

r[server.watch.respect-excludes]
The file watcher MUST respect `exclude` patterns from the configuration, not triggering rebuilds for files matching exclude patterns even if they match include patterns.

r[server.watch.config-file]
The file watcher MUST watch the configuration file itself (`.config/tracey/config.kdl`) for changes, triggering a rebuild when configuration changes.

r[server.watch.debounce]
File change events MUST be debounced (default: 200ms) to avoid excessive recomputation during rapid edits.

### State Management

r[server.state.shared]
Both HTTP and MCP modes MUST use the same underlying coverage computation and state.

r[server.state.version]
The server MUST maintain a version identifier that changes when any source data changes.

## Validation

Tracey validates the integrity and quality of requirement definitions and references.

r[validation.broken-refs]
The system MUST detect and report references to non-existent requirement IDs in implementation and verification comments.

r[validation.naming]
The system MUST validate that requirement IDs follow the configured naming convention (e.g., section.subsection.name format).

r[validation.circular-deps]
The system MUST detect circular dependencies if requirements reference each other, preventing infinite loops in dependency resolution.

r[validation.orphaned]
The system MUST identify requirements that are defined in specs but never referenced in implementation or verification comments.

r[validation.duplicates]
The system MUST detect duplicate requirement IDs across all spec files.

## MCP Server

The MCP server exposes tracey functionality as tools for AI assistants.

### Response Format

r[mcp.response.header]
Every MCP tool response MUST begin with a status line showing current coverage for all spec/implementation combinations.

> r[mcp.response.header-format]
> The header MUST follow this format:
>
> ```
> tracey | spec1/impl1: 72% | spec2/impl2: 45%
> ```

r[mcp.response.delta]
Every MCP tool response MUST include a delta section showing changes since the last query in this session.

> r[mcp.response.delta-format]
> The delta section MUST follow this format:
>
> ```
> Since last query:
>   ✓ req.id.one → src/file.rs:42
>   ✓ req.id.two → src/other.rs:67
> ```
>
> If no changes occurred, display: `(no changes since last query)`

r[mcp.response.hints]
Tool responses SHOULD include hints showing how to drill down or query further.

r[mcp.response.text]
Tool responses MUST be formatted as human-readable text/markdown, not JSON.

### Spec/Implementation Selection

r[mcp.select.single]
When only one spec and one implementation are configured, tools MUST use them by default without requiring explicit selection.

r[mcp.select.spec-only]
When a spec has only one implementation, specifying just the spec name MUST be sufficient.

r[mcp.select.full]
The full `spec/impl` syntax MUST be supported for explicit selection when multiple options exist.

r[mcp.select.ambiguous]
When selection is ambiguous and not provided, tools MUST return an error listing available options.

### Tools

r[mcp.tool.status]
The `tracey_status` tool MUST return a coverage overview and list available query commands.

r[mcp.tool.uncovered]
The `tracey_uncovered` tool MUST return requirements without `impl` references, grouped by markdown section.

r[mcp.tool.uncovered-section]
The `tracey_uncovered` tool MUST support a `--section` parameter to filter to a specific section.

r[mcp.tool.untested]
The `tracey_untested` tool MUST return requirements without `verify` references, grouped by markdown section.

r[mcp.tool.untested-section]
The `tracey_untested` tool MUST support a `--section` parameter to filter to a specific section.

r[mcp.tool.unmapped]
The `tracey_unmapped` tool MUST return a tree view of source files with coverage percentages.

> r[mcp.tool.unmapped-tree]
> The tree view MUST use ASCII art formatting similar to the `tree` command:
>
> ```
> src/
> ├── channel/           82% ████████░░
> │   ├── flow.rs        95% █████████░
> │   └── close.rs       45% ████░░░░░░
> └── error/             34% ███░░░░░░░
> ```

r[mcp.tool.unmapped-zoom]
The `tracey_unmapped` tool MUST accept an optional path parameter to zoom into a specific directory or file.

r[mcp.tool.unmapped-file]
When zoomed into a specific file, `tracey_unmapped` MUST list individual unmapped code units with line numbers.

r[mcp.tool.req]
The `tracey_req` tool MUST return the full text of a requirement and all references to it.

### Configuration Tools

r[mcp.config.exclude]
The `tracey_config_exclude` tool MUST allow adding exclude patterns to filter out files from scanning.

r[mcp.config.include]
The `tracey_config_include` tool MUST allow adding include patterns to expand the set of scanned files.

r[mcp.config.list]
The `tracey_config` tool MUST display the current configuration for all specs and implementations.

r[mcp.config.persist]
Configuration changes made via MCP tools MUST be persisted to the configuration file.

### Progressive Discovery

r[mcp.discovery.overview-first]
Initial queries SHOULD return summarized results with counts per section/directory.

r[mcp.discovery.drill-down]
Responses MUST include hints showing how to query for more specific results.

r[mcp.discovery.pagination]
Large result sets SHOULD be paginated with hints showing how to retrieve more results.

### Validation Tools

r[mcp.validation.check]
The `tracey_validate` tool MUST run all validation checks and return a report of issues found (broken refs, naming violations, circular deps, orphaned requirements, duplicates).

r[dashboard.validation.display]
The dashboard MUST display validation errors prominently, with links to the problematic locations.

r[dashboard.validation.continuous]
The dashboard SHOULD run validation continuously and update the UI when new issues are detected.

### Query Tools

r[mcp.query.search]
The `tracey_search` tool MUST support keyword search across requirement text and IDs, returning matching requirements with their definitions and references.

r[mcp.query.file-reqs]
The `tracey_file_reqs` tool MUST return all requirements referenced in a specific source file, grouped by reference type (impl/verify).

r[mcp.query.priority]
The `tracey_priority` tool MUST suggest which uncovered requirements to implement next, prioritizing by section completeness and requirement dependencies.

r[dashboard.query.search]
The dashboard MUST provide a search interface for finding requirements by keyword in their text or ID.

r[dashboard.query.file-reqs]
The dashboard MUST show all requirements referenced by a specific file when viewing file details.

## Dashboard In-Browser Editing

The dashboard provides inline editing of requirements directly in the browser, enabling rapid iteration on specification documents without leaving the review interface.

### Byte Range Tracking

r[dashboard.editing.byte-range.req-span]
Each requirement definition MUST track its byte range in the source markdown file, storing both the start offset and end offset (inclusive of the requirement marker line and all content lines).

r[dashboard.editing.byte-range.marker-and-content]
The byte range MUST include the requirement marker line (`r[req.id]`) plus all following content lines until a blank line, another requirement, or a heading is encountered.

r[dashboard.editing.byte-range.attribute]
The requirement's HTML container MUST include a `data-br="start-end"` attribute containing the byte range, enabling the editor to locate and update the exact markdown source.

Example:
```html
<div class="req-container" data-br="1234-1456">
  <!-- requirement content -->
</div>
```

### Edit Badge and Activation

r[dashboard.editing.badge.display]
Each requirement MUST display an "Edit" badge in its header, styled as a low-emphasis button with a pencil icon to indicate editability without demanding attention.

r[dashboard.editing.badge.appearance]
The Edit badge MUST use muted colors (gray) by default and accent colors on hover, and MUST be larger and more prominent than reference badges to make editing discoverable.

r[dashboard.editing.activation.click]
Clicking the Edit badge MUST activate inline editing mode for that requirement, replacing the rendered requirement with the inline editor.

### Copy Requirement ID

r[dashboard.editing.copy.button]
Each requirement MUST display a "Copy" button or icon in its header that copies the requirement ID to the clipboard when clicked.

r[dashboard.editing.copy.format]
The copy button MUST copy only the requirement ID (e.g., `dashboard.editing.copy.button`) without any prefix or brackets, ready for use in implementation references.

r[dashboard.editing.copy.feedback]
After copying, the button MUST provide visual feedback (e.g., brief color change, checkmark icon, or "Copied!" tooltip) to confirm the action succeeded.

### Implementation Preview Modal

r[dashboard.impl-preview.modal]
When viewing the specification tab and clicking on an implementation reference badge, the dashboard MUST open a modal showing the source code without navigating away from the spec tab.

r[dashboard.impl-preview.scroll-highlight]
The implementation preview modal MUST automatically scroll to the referenced line number and highlight it for easy identification.

r[dashboard.impl-preview.open-in-sources]
The modal MUST include an "Open in sources" button that navigates to the full sources view when clicked, allowing users to see more context or make edits.

r[dashboard.impl-preview.stay-in-spec]
Clicking an implementation reference badge MUST NOT automatically switch to the sources tab - it MUST show the preview modal while keeping the user in the specification view.

r[dashboard.impl-preview.rationale]
The preview modal allows quick inspection of implementation code without losing context in the specification, reducing cognitive load when reviewing how requirements are implemented.

### Inline Editor Interface

r[dashboard.editing.inline.fullwidth]
The inline editor MUST display as a full-width editor showing only the markdown source, replacing the entire requirement block during editing.

r[dashboard.editing.inline.vim-mode]
The editor MUST support vim keybindings via CodeMirror's vim extension, with a visible "VIM" indicator in the editor header.

r[dashboard.editing.inline.codemirror]
The editor MUST use CodeMirror 6 with markdown language support, providing syntax highlighting, line wrapping, and proper monospace font rendering.

r[dashboard.editing.inline.header]
The editor header MUST display: an "Edit Requirement" label, vim mode indicator, and the source file path for context.

r[dashboard.editing.inline.dimensions]
The editor MUST have a minimum height of 300px and maximum height of 600px to accommodate short requirements while preventing excessive vertical space usage.

### Git Safety

r[dashboard.editing.git.check-required]
The dashboard MUST check if a file is in a git repository before allowing inline editing, refusing to edit files that are not tracked by git.

r[dashboard.editing.git.api]
The server MUST provide a `GET /api/check-git?path=X` endpoint that returns `{in_git: boolean}` indicating whether the specified file is within a git repository.

r[dashboard.editing.git.error-message]
When a user attempts to edit a file not in a git repository, the dashboard MUST display a clear error message: "This file is not in a git repository. Tracey requires git for safe editing."

r[dashboard.editing.git.rationale]
Git requirement provides a safety net for inline editing: users can review changes with `git diff`, revert mistakes with `git checkout`, and maintain edit history without additional undo/redo implementation.

### Byte Range Operations

r[dashboard.editing.api.fetch-range]
The server MUST provide a `GET /api/file-range?path=X&start=N&end=M` endpoint that returns the exact bytes from the specified range as UTF-8 text, along with a BLAKE3 hash of the entire file.

r[dashboard.editing.api.fetch-range-response]
The fetch-range endpoint MUST return `{content: string, start: number, end: number, file_hash: string}` where `file_hash` is the hexadecimal BLAKE3 hash of the entire file contents.

r[dashboard.editing.api.update-range]
The server MUST provide a `PATCH /api/file-range` endpoint accepting `{path, start, end, content, file_hash}` that replaces the specified byte range with new content only if the file hash matches.

r[dashboard.editing.api.update-range-response]
The update-range endpoint MUST return `{content: string, start: number, end: number, file_hash: string}` on success, where `file_hash` is the BLAKE3 hash of the file after the update.

r[dashboard.editing.api.hash-conflict]
If the provided `file_hash` does not match the current file's BLAKE3 hash, the server MUST return HTTP 409 Conflict with an error message indicating the file has changed since it was loaded.

r[dashboard.editing.api.range-validation]
The byte range endpoints MUST validate that `start < end` and `end <= file_length`, returning appropriate error codes for invalid ranges.

r[dashboard.editing.api.utf8-validation]
The fetch-range endpoint MUST validate that the extracted bytes form valid UTF-8 text, returning an error if the range splits a multi-byte character.

r[dashboard.editing.hash.rationale]
Hash-based conflict detection prevents race conditions where the file changes between loading the editor and saving, ensuring users don't accidentally overwrite concurrent modifications.

### Save and Cancel Workflow

r[dashboard.editing.save.patch-file]
Clicking Save MUST send a PATCH request with the new content, update the file, and close the inline editor on success.

r[dashboard.editing.save.error-handling]
Save errors MUST be displayed inline in the editor footer without closing the editor, allowing the user to retry or cancel.

r[dashboard.editing.cancel.discard]
Clicking Cancel MUST discard all changes and close the inline editor without modifying the file.

r[dashboard.editing.cancel.escape]
Pressing Escape MUST cancel editing and close the inline editor (when not in vim insert mode).

### File Watching and Reload

r[dashboard.editing.reload.auto-detect]
After a successful save, the file watcher MUST detect the change and trigger a dashboard rebuild automatically.

r[dashboard.editing.reload.live-update]
The dashboard MUST connect to the WebSocket endpoint to receive notifications of rebuilds and reload the page content when a new version is available, preserving scroll position.

r[dashboard.editing.reload.smooth]
The reload MUST be visually smooth, with no jarring page transitions or loss of context (scroll position maintained).

### Keyboard Navigation

r[dashboard.editing.keyboard.navigation]
The specification view MUST support keyboard navigation between requirements using `j` (next) and `k` (previous) keys.

> r[dashboard.editing.keyboard.next-req]
> Pressing `j` MUST scroll to and focus the next requirement in the document, wrapping to the first requirement when at the end.

> r[dashboard.editing.keyboard.prev-req]
> Pressing `k` MUST scroll to and focus the previous requirement in the document, wrapping to the last requirement when at the start.

> r[dashboard.editing.keyboard.open-editor]
> Pressing `e` MUST open the inline editor for the currently focused requirement.

> r[dashboard.editing.keyboard.visual-focus]
> The currently focused requirement MUST have a visual indicator (highlight, border, or similar) to show which requirement is active for keyboard operations.

> r[dashboard.editing.keyboard.scope]
> Keyboard shortcuts MUST only be active when not typing in an input field, textarea, or editor to avoid conflicts with normal text entry.

### Future: Add Requirement

r[dashboard.editing.add.button status=draft]
The dashboard SHOULD provide an "Add Requirement" button that inserts a new requirement template at appropriate locations (end of sections, after existing requirements).

r[dashboard.editing.add.template status=draft]
The template MUST be pre-filled as a blockquote with `r[|]` where `|` represents the cursor position, ready for the user to type the requirement ID.

Example template:
```markdown
> r[|]
>
```

r[dashboard.editing.add.cursor-position status=draft]
The cursor MUST be positioned inside the brackets after `r[` to enable immediate typing of the requirement ID, followed by tab to move to the content line.
