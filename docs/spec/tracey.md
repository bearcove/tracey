# Tracey Specification

This document specifies the behavior of tracey, a tool for measuring spec coverage in Rust codebases.

## Rule References in Rust Code

Tracey extracts rule references from Rust source code comments.

### Basic Syntax

r[ref.syntax.brackets]
A rule reference MUST be enclosed in square brackets within a Rust comment.

r[ref.syntax.rule-id]
A rule ID MUST consist of one or more segments separated by dots. Each segment MUST contain only alphanumeric characters, hyphens, or underscores.

r[ref.syntax.verb]
A rule reference MAY include a verb prefix before the rule ID, separated by a space.

### Supported Verbs

r[ref.verb.define]
Tracey MUST interpret the `define` verb as indicating where a requirement is defined (typically in specs/docs).

r[ref.verb.impl]
Tracey MUST interpret the `impl` verb as indicating that the code implements the referenced rule.

r[ref.verb.verify]
Tracey MUST interpret the `verify` verb as indicating that the code tests or verifies the referenced rule.

r[ref.verb.depends]
Tracey MUST interpret the `depends` verb as indicating a strict dependency — the code must be rechecked if the referenced rule changes.

r[ref.verb.related]
Tracey MUST interpret the `related` verb as indicating a loose connection, shown when reviewing related code.

r[ref.verb.default]
When no verb is provided, the reference SHOULD be treated as an `impl` reference.

r[ref.verb.unknown]
When an unrecognized verb is encountered, tracey MUST emit a warning but SHOULD still extract the rule reference.

### Comment Types

r[ref.comments.line]
Rule references MUST be recognized in line comments (`//`).

r[ref.comments.block]
Rule references MUST be recognized in block comments (`/* */`).

r[ref.comments.doc]
Rule references MUST be recognized in doc comments (`///` and `//!`).

### Source Location Tracking

r[ref.span.offset]
Each extracted rule reference MUST include the byte offset of its location in the source file.

r[ref.span.length]
Each extracted rule reference MUST include the byte length of the reference.

r[ref.span.file]
Each extracted rule reference MUST include the path to the source file.

## Rule Definitions in Markdown

Tracey can extract rule definitions from markdown specification documents.

### Markdown Rule Syntax

r[markdown.syntax.marker]
A rule definition MUST be written as `r[rule.id]` on its own line in the markdown.

r[markdown.syntax.standalone]
The rule marker MUST appear on its own line (possibly with leading/trailing whitespace).

r[markdown.syntax.inline-ignored]
Rule markers that appear inline within other text MUST be treated as regular text, not rule definitions.

r[markdown.syntax.blockquote]
A rule definition MAY be written inside a blockquote (`> r[rule.id]`) to allow multi-paragraph content including code blocks.

### Duplicate Detection

r[markdown.duplicates.same-file]
If the same rule ID appears multiple times in a single markdown file, tracey MUST report an error.

r[markdown.duplicates.cross-file]
If the same rule ID appears in multiple markdown files, tracey MUST report an error when merging manifests.

### HTML Output

r[markdown.html.div]
When transforming markdown, each rule marker MUST be replaced with a `<div>` element with class `rule`.

r[markdown.html.anchor]
The generated div MUST have an `id` attribute in the format `r-{rule.id}` for linking.

r[markdown.html.link]
The generated div MUST contain a link (`<a>`) pointing to its own anchor.

r[markdown.html.wbr]
Dots in the displayed rule ID SHOULD be followed by `<wbr>` elements to allow line breaking.

## Coverage Computation

r[coverage.compute.percentage]
Coverage percentage MUST be calculated as (covered rules / total rules) * 100.

r[coverage.compute.covered]
Tracey MUST consider a rule covered if at least one reference to it exists in the scanned source files.

r[coverage.compute.uncovered]
Rules in the manifest with no references MUST be reported as uncovered.

r[coverage.compute.invalid]
References to rule IDs not present in the manifest MUST be reported as invalid.

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
>     name "spec-name"
>     rules_glob "docs/spec/**/*.md"
>
>     impl {
>         lang "rust"
>         include "crates/**/*.rs"
>         exclude "target/**"
>     }
> }
> ```

r[config.spec.name]
Each spec configuration MUST have a `name` child node with the spec name as its argument.

r[config.spec.rules-glob]
Each spec configuration MUST have a `rules_glob` child node specifying a glob pattern for markdown files containing rule definitions.

r[config.impl.lang]
Each impl configuration MUST have a `lang` child node identifying the implementation language.

r[config.impl.include]
Each impl configuration MAY have one or more `include` child nodes specifying glob patterns for source files to scan.

r[config.impl.exclude]
Each impl configuration MAY have one or more `exclude` child nodes specifying glob patterns for source files to exclude.

## File Walking

r[walk.gitignore]
File walking MUST respect `.gitignore` rules.

r[walk.default-include]
When no include patterns are specified, tracey MUST default to `**/*.rs`.

## Dashboard

Tracey provides a web-based dashboard for browsing specifications, viewing coverage, and navigating source code.

### URL Scheme

r[dashboard.url.structure]
Dashboard URLs MUST follow the structure `/:specName/:impl/:view` where `specName` is the name of a configured spec and `impl` is an implementation name.

r[dashboard.url.spec-view]
The specification view MUST be accessible at `/:specName/:impl/spec` with optional heading hash fragment `/:specName/:impl/spec#:headingSlug`.

r[dashboard.url.coverage-view]
The coverage view MUST be accessible at `/:specName/:impl/coverage` with optional query parameters `?filter=impl|verify` and `?level=must|should|may`.

r[dashboard.url.sources-view]
The sources view MUST be accessible at `/:specName/:impl/sources` with optional file and line parameters `/:specName/:impl/sources/:filePath::lineNumber`.

r[dashboard.url.context]
Source URLs MAY include a `?context=:ruleId` query parameter to show rule context in the sidebar.

r[dashboard.url.root-redirect]
Navigating to `/` MUST redirect to `/:defaultSpec/:defaultImpl/spec` where `defaultSpec` is the first configured spec and `defaultImpl` is its first implementation.

r[dashboard.url.invalid-spec]
Navigating to an invalid spec name SHOULD redirect to the first valid spec or display an error.

### API Endpoints

r[dashboard.api.config]
The `/api/config` endpoint MUST return the project configuration including `projectRoot` and `specs` array.

r[dashboard.api.spec]
The `/api/spec?spec=:specName&impl=:impl` endpoint MUST return the rendered HTML and outline for the named spec and implementation.

r[dashboard.api.forward]
The `/api/forward?spec=:specName&impl=:impl` endpoint MUST return the forward mapping (rules to file references) for the specified implementation.

r[dashboard.api.reverse]
The `/api/reverse?spec=:specName&impl=:impl` endpoint MUST return the reverse mapping (files to rule references) with coverage statistics for the specified implementation.

r[dashboard.api.file]
The `/api/file?spec=:specName&impl=:impl&path=:filePath` endpoint MUST return the file content, syntax-highlighted HTML, and code unit annotations.

r[dashboard.api.version]
The `/api/version` endpoint MUST return a version string that changes when any source data changes.

r[dashboard.api.version-polling]
The dashboard SHOULD poll `/api/version` and refetch data when the version changes.

### Link Generation

r[dashboard.links.spec-aware]
All links generated in rendered markdown MUST include the spec name and implementation as the first two path segments.

r[dashboard.links.rule-links]
Rule ID badges MUST link to `/:specName/:impl/spec?rule=:ruleId` to navigate to the rule in the specification.

r[dashboard.links.impl-refs]
Implementation reference badges MUST link to `/:specName/:impl/sources/:filePath::line?context=:ruleId`.

r[dashboard.links.verify-refs]
Verification/test reference badges MUST link to `/:specName/:impl/sources/:filePath::line?context=:ruleId`.

r[dashboard.links.heading-links]
Heading links in the outline MUST link to `/:specName/:impl/spec#:headingSlug`.

### Specification View

r[dashboard.spec.outline]
The specification view MUST display a collapsible outline tree of headings in a sidebar.

r[dashboard.spec.outline-coverage]
Each outline heading SHOULD display a coverage indicator showing the ratio of covered rules within that section.

r[dashboard.spec.content]
The specification view MUST display the rendered markdown content with rule containers.

r[dashboard.spec.rule-highlight]
When navigating to a rule via URL parameter `?rule=:ruleId`, the rule container MUST be highlighted and scrolled into view.

r[dashboard.spec.heading-scroll]
When navigating to a heading via URL path, the heading MUST be scrolled into view.

r[dashboard.spec.switcher]
When multiple specs are configured, the specification view MUST display a spec switcher UI.

r[dashboard.spec.switcher-single]
When only one spec is configured, the spec switcher SHOULD be hidden.

### Coverage View

r[dashboard.coverage.table]
The coverage view MUST display a table of all rules with their coverage status.

r[dashboard.coverage.filter-type]
The coverage view MUST support filtering by reference type (impl, verify, or all).

r[dashboard.coverage.filter-level]
The coverage view MUST support filtering by RFC 2119 level (MUST, SHOULD, MAY, or all).

r[dashboard.coverage.stats]
The coverage view MUST display summary statistics including total rules, covered count, and coverage percentage.

r[dashboard.coverage.rule-links]
Each rule in the coverage table MUST link to the rule in the specification view.

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
Lines containing rule references MUST be annotated with indicators showing which rules are referenced.

r[dashboard.sources.line-highlight]
When navigating to a specific line, that line MUST be highlighted and scrolled into view.

r[dashboard.sources.rule-context]
When a `?context=:ruleId` parameter is present, the sidebar MUST display the rule details and all its references.

r[dashboard.sources.editor-open]
Clicking a line number SHOULD open the file at that line in the configured editor.

### Search

r[dashboard.search.modal]
The search modal MUST be openable via keyboard shortcut (Cmd+K on Mac, Ctrl+K elsewhere).

r[dashboard.search.rules]
Search MUST support finding rules by ID or text content.

r[dashboard.search.files]
Search MUST support finding files by path.

r[dashboard.search.navigation]
Selecting a search result MUST navigate to the appropriate view (spec for rules, sources for files).

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
