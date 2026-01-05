# Handoff: Fix Coverage Table Missing Rule Text

## Completed
- Improved sidebar outline tree indentation (vertical guides, toggle overlap, spacing)
- Changed frontend `Rule` type from `text?: string` to `html?: string`
- Updated coverage.tsx to use `rule.html` instead of `rule.text`

## Active Work
### What We're Working On
The coverage table at `/rust/coverage` shows rule IDs but no rule descriptions/text below them.

### Current Status
Root cause identified: `bearmark::extract_rules_only()` extracts rule IDs but leaves `html` field empty. User says "extract_rules_only is garbage and should not exist" and to delete it.

The backend `ApiRule` struct has `html: String` but it's populated from `rule_def.html.clone()` which is empty because `extract_rules_only` doesn't render HTML.

### Context
- `ApiRule` in `serve.rs:91` has both `html` and newly added `text` field
- Rules come from `load_rules_from_glob()` in `main.rs` which uses `bearmark::extract_rules_only()`
- `RuleDefinition` (from bearmark) has: `id`, `text` (raw markdown), `html` (empty from extract_rules_only)
- bearmark is at `../dodeca/crates/bearmark/`
- The full render path uses `TraceyRuleHandler` which DOES populate HTML but that's only for spec view

### Next Steps
1. Delete `extract_rules_only` usage
2. Use full `bearmark::render()` to get rules with populated HTML
3. Or: Use `rule_def.text` (raw markdown) and render with `renderRuleText()` on frontend

### Tracking
- Branch: `svg-arc-indicators`

## Key Files
- `crates/tracey/src/serve.rs` - ApiRule struct, data loading
- `crates/tracey/src/main.rs:99` - `load_rules_from_glob()` uses `extract_rules_only`
- `crates/tracey/dashboard/src/views/coverage.tsx` - renders rule table
- `crates/tracey/dashboard/src/types.ts:34` - Rule interface
- `../dodeca/crates/bearmark/src/render.rs` - extract_rules_only lives here

## Blockers/Gotchas
- User explicitly said `extract_rules_only` is garbage and should be deleted
- bearmark is in sibling repo `../dodeca/crates/bearmark/`
- The `html` field from extract_rules_only is ALWAYS empty - that's the bug

## Bootstrap
```bash
git status && cargo check -p tracey
```
