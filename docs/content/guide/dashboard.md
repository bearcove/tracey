+++
title = "Dashboard"
weight = 5
+++

Tracey includes a web dashboard for browsing specs, exploring coverage, and navigating source code.

## Starting the dashboard

```bash
tracey web --open
```

This starts the dashboard at `http://localhost:3000` and opens it in your browser. Use `--port` to change the port:

```bash
tracey web --port 8080
```

The dashboard auto-starts the tracey daemon if it isn't already running.

## Views

The dashboard has three main views, accessible via tabs in the header:

### Specification view

The default view. Shows your rendered spec with coverage badges on each requirement:

- **Green badge** — requirement has implementation references
- **No badge** — requirement is uncovered

The sidebar shows a collapsible outline of your spec's headings, each with a coverage indicator showing the ratio of covered requirements in that section. The header displays overall coverage percentages for both implementation and verification.

Click a requirement's copy button to copy its ID to the clipboard (for pasting into code annotations). Click an implementation reference badge to preview the source code in a modal without leaving the spec view.

### Coverage view

A filterable table of all requirements and their coverage status.

**Filters:**
- **Type** — show only `impl` references, only `verify` references, or all
- **Level** — filter by RFC 2119 keywords (MUST, SHOULD, MAY, or all)

The header shows summary statistics: total requirements, covered count, and coverage percentage. Each requirement links to its definition in the spec view, and each reference links to the source location.

### Sources view

A file tree in the sidebar showing per-file and per-directory coverage percentages. Select a file to see syntax-highlighted source code with requirement annotations marked on the relevant lines.

Click a line number to open the file at that line in your editor. When navigating to a source location from another view, the URL includes a `?context={reqId}` parameter that shows the requirement details in the sidebar.

## Keyboard shortcuts

The spec view supports vim-style keyboard navigation:

| Key | Action |
|-----|--------|
| `j` | Next requirement |
| `k` | Previous requirement |
| `J` | Next uncovered requirement |
| `K` | Previous uncovered requirement |
| `e` | Edit the focused requirement |
| `/` | Open search |
| `gg` | Scroll to top |
| `G` | Scroll to bottom |
| `yy` | Copy requirement ID and text |
| `yl` | Copy requirement ID only |

Keyboard shortcuts are disabled when typing in an input field or editor.

## Search

Open search with `Cmd+K` (Mac) or `Ctrl+K` (elsewhere). Search finds:

- **Requirements** by ID or text content (shown first in results)
- **Files** by path

Select a result to navigate to the spec view (for requirements) or sources view (for files).

## Inline editing

Click the Edit badge on any requirement to open an inline editor. The editor uses CodeMirror with markdown syntax highlighting and vim keybindings (indicated by a "VIM" badge in the editor header).

**Save** writes the changes to the spec file on disk. **Cancel** (or `:q` in vim mode) discards changes.

Tracey requires the spec file to be in a git repository before allowing edits — this provides a safety net for reverting changes with `git checkout` or reviewing them with `git diff`.

After saving, the file watcher detects the change and rebuilds the dashboard automatically.

## Implementation preview

When viewing the spec, clicking an implementation or verification reference badge opens a modal showing the source code at that location. This lets you inspect implementing code without switching to the sources tab. Click "Open in sources" in the modal to see full context.

## Live updates

The dashboard connects to the daemon via WebSocket and refreshes automatically when files change. Edit a spec file or add an annotation to source code, save, and the dashboard updates within a moment — no manual reload needed.

## URL structure

Dashboard URLs follow the pattern `/{specName}/{impl}/{view}`:

- `/{specName}/{impl}/spec` — specification view
- `/{specName}/{impl}/coverage` — coverage view
- `/{specName}/{impl}/sources/{filePath}:{line}` — sources view at a specific location

Navigating to `/` redirects to the first configured spec's specification view.
