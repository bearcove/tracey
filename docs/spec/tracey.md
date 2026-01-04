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

## Manifest Format

r[manifest.format.json]
The rules manifest MUST be valid JSON.

r[manifest.format.rules-key]
The manifest MUST have a top-level `rules` object.

r[manifest.format.rule-entry]
Each rule entry MUST be keyed by the rule ID and MUST contain a `url` field.

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

r[config.spec.name]
Each spec configuration MUST have a `name` field.

r[config.spec.source]
Each spec configuration MUST have exactly one rules source: `rules_url`, `rules_file`, or `rules_glob`.

r[config.spec.include]
The `include` patterns MUST filter which source files are scanned.

r[config.spec.exclude]
The `exclude` patterns MUST exclude matching source files from scanning.

## File Walking

r[walk.gitignore]
File walking MUST respect `.gitignore` rules.

r[walk.default-include]
When no include patterns are specified, tracey MUST default to `**/*.rs`.

r[walk.default-exclude]
When no exclude patterns are specified, tracey MUST default to excluding `target/**`.
