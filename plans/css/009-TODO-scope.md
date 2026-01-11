# Plan 009: @scope for Component Isolation

## Overview

CSS `@scope` provides true component-style isolation without the need for BEM naming conventions or CSS-in-JS. It limits style application to a specific DOM subtree and can define both upper and lower boundaries.

## Current Problems

### 1. Generic Class Name Conflicts

```css
/* These could conflict if used in different contexts */
.modal-content .header { ... }
.sidebar .header { ... }
.content-header { ... }  /* Renamed to avoid conflict */
```

### 2. Deep Selector Chains

```css
/* Long selectors to ensure specificity */
.search-modal .search-modal-result .search-modal-result-header { ... }
.inline-editor .inline-editor-header .inline-editor-label { ... }
```

### 3. Leaky Styles

```css
/* Markdown styles might leak into nested components */
.markdown code { ... }  /* Affects code in nested modals too */
```

## @scope Syntax

```css
@scope (.component) {
  /* Styles only apply within .component */
  .header { ... }
  .body { ... }
}

@scope (.component) to (.nested-boundary) {
  /* Styles apply within .component but stop at .nested-boundary */
  p { ... }  /* Won't style <p> inside .nested-boundary */
}
```

## Refactoring Plan

### Phase 1: Modal Components

**Before:**
```css
.modal-overlay { ... }
.modal-content { ... }
.modal-content.editor-modal { ... }
.modal-header { ... }
.modal-header h3 { ... }
.modal-vim-indicator { ... }
.modal-close { ... }
.modal-body { ... }
.modal-loading { ... }
.modal-error { ... }
.modal-footer { ... }
.modal-info { ... }
.modal-info code { ... }
.modal-range { ... }
.modal-actions { ... }
.modal-btn { ... }
.modal-btn:disabled { ... }
.modal-btn-cancel { ... }
.modal-btn-save { ... }
.modal-btn-primary { ... }
```

**After:**
```css
@scope (.modal) {
  :scope {
    /* .modal itself */
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.6);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 10000;
    padding: 2rem;
  }

  .content {
    background: var(--bg-secondary);
    border: 1px solid var(--border);
    border-radius: 8px;
    width: 900px;
    max-width: 100%;
    max-height: 90vh;
    display: flex;
    flex-direction: column;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.4);

    &.editor { width: 1200px; }
  }

  .header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 1rem 1.5rem;
    border-bottom: 1px solid var(--border);

    h3 {
      margin: 0;
      font-size: 1.1rem;
      font-weight: 600;
    }
  }

  .vim-indicator {
    font-size: 0.7rem;
    padding: 0.2rem 0.5rem;
    background: var(--green-dim);
    color: var(--green);
    border-radius: 3px;
    font-weight: 600;
    letter-spacing: 0.05em;
    margin-left: auto;
    margin-right: 0.5rem;
  }

  .close-btn {
    background: none;
    border: none;
    font-size: 1.5rem;
    color: var(--fg-muted);
    cursor: pointer;
    padding: 0;
    width: 2rem;
    height: 2rem;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 4px;

    &:hover {
      background: var(--hover);
      color: var(--fg);
    }
  }

  .body {
    flex: 1;
    overflow: hidden;
    display: flex;
    flex-direction: column;
    min-height: 400px;
  }

  .loading, .error {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--fg-muted);
    font-size: 0.9rem;
  }

  .error { color: var(--red); }

  .footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 1rem 1.5rem;
    border-top: 1px solid var(--border);
    gap: 1rem;
  }

  .info {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    font-size: 0.8rem;
    color: var(--fg-muted);
    flex: 1;
    min-width: 0;

    code {
      background: var(--bg);
      padding: 0.2rem 0.4rem;
      border-radius: 3px;
      font-size: 0.75rem;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
  }

  .actions {
    display: flex;
    gap: 0.75rem;
  }

  .btn {
    padding: 0.5rem 1rem;
    border-radius: 6px;
    border: 1px solid var(--border);
    font-family: inherit;
    font-size: 0.85rem;
    cursor: pointer;

    &:disabled {
      opacity: 0.5;
      cursor: not-allowed;
    }

    &.cancel {
      background: var(--bg);
      color: var(--fg);
      &:hover:not(:disabled) { background: var(--hover); }
    }

    &.save, &.primary {
      background: var(--accent);
      color: white;
      border-color: var(--accent);
      &:hover:not(:disabled) { filter: brightness(1.1); }
    }
  }
}
```

### Phase 2: Search Modal with Boundary

**Using `to` for lower boundary:**
```css
@scope (.search-modal) to (.search-result-code) {
  /* Styles won't leak into code blocks */
  mark {
    background: var(--yellow-dim);
    color: var(--yellow);
    padding: 0.1em 0.2em;
    border-radius: 2px;
  }
}

@scope (.search-modal) {
  :scope {
    background: var(--bg-secondary);
    border: 1px solid var(--border);
    border-radius: 8px;
    width: 600px;
    max-width: 90vw;
    max-height: 70vh;
    display: flex;
    flex-direction: column;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.3);
  }

  .input-wrapper {
    padding: 1rem;
    border-bottom: 1px solid var(--border);

    input {
      width: 100%;
      padding: 0.75rem 1rem;
      background: var(--bg);
      border: 1px solid var(--border);
      border-radius: 6px;
      font-size: 1rem;
      color: var(--fg);

      &:focus {
        outline: none;
        border-color: var(--accent);
        box-shadow: 0 0 0 3px var(--accent-dim);
      }
    }
  }

  .results {
    flex: 1;
    overflow-y: auto;
  }

  .result {
    padding: 0.75rem 1rem;
    border-bottom: 1px solid var(--border);
    cursor: pointer;

    &:hover { background: var(--hover); }
    &.selected { background: var(--accent-dim); }
  }

  .result-header {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    margin-bottom: 0.25rem;
  }

  .result-icon {
    width: 1.1rem;
    height: 1.1rem;
    &.source { color: var(--green); }
    &.rule { color: var(--accent); }
  }

  .hint {
    padding: 0.75rem 1rem;
    border-top: 1px solid var(--border);
    font-size: 0.75rem;
    color: var(--fg-dim);
    display: flex;
    gap: 1rem;

    kbd {
      background: var(--bg);
      border: 1px solid var(--border);
      border-radius: 3px;
      padding: 0.1rem 0.35rem;
      font-size: 0.7rem;
    }
  }
}
```

### Phase 3: Inline Editor

```css
@scope (.inline-editor) {
  :scope {
    border: 2px solid var(--accent);
    border-radius: 6px;
    background: var(--bg-secondary);
    margin: 1rem 0;
    overflow: hidden;
  }

  .header {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 0.5rem 0.75rem;
    background: var(--accent-dim);
    border-bottom: 1px solid var(--border);
    font-size: 0.8rem;
  }

  .label {
    font-weight: 600;
    color: var(--accent);
  }

  .vim-badge {
    font-size: 0.65rem;
    padding: 0.15rem 0.4rem;
    background: var(--green-dim);
    color: var(--green);
    border-radius: 3px;
    font-weight: 600;
    letter-spacing: 0.05em;
  }

  .path {
    color: var(--fg-dim);
    font-family: var(--font-mono);
    font-size: 0.75rem;
    margin-left: auto;
  }

  .content {
    min-height: 400px;
    max-height: 600px;
    overflow: auto;
    background: var(--bg);
  }

  .code-wrapper {
    height: 100%;
    background: var(--bg);

    .cm-editor { height: 100%; }
  }

  .preview {
    padding: 1.5rem;
    background: var(--bg);
    min-height: 400px;
  }

  .footer {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
    padding: 0.75rem;
    border-top: 1px solid var(--border);
    background: var(--bg-secondary);
  }

  .btn {
    padding: 0.4rem 0.75rem;
    border-radius: 4px;
    border: 1px solid var(--border);
    font-family: inherit;
    font-size: 0.8rem;
    cursor: pointer;

    &:disabled { opacity: 0.5; cursor: not-allowed; }
    &.cancel {
      background: var(--bg);
      color: var(--fg);
      &:hover:not(:disabled) { background: var(--hover); }
    }
    &.save {
      background: var(--accent);
      color: white;
      border-color: var(--accent);
      &:hover:not(:disabled) { filter: brightness(1.1); }
    }
  }

  .loading, .error {
    padding: 1rem;
    text-align: center;
    color: var(--fg-muted);
    font-size: 0.85rem;
  }
  .error { color: var(--red); }
}
```

### Phase 4: Markdown with Boundaries

```css
/* Stop markdown styles from leaking into embedded components */
@scope (.markdown) to (.code-view, .inline-editor, .rule-block, .req-container) {
  h1, h2, h3, h4 {
    margin: 1.5rem 0 0.75rem;
    color: var(--fg-heading);
    cursor: pointer;
  }

  p { margin: 1rem 0; }

  a {
    color: var(--accent);
    text-decoration: none;
    &:hover { text-decoration: underline; }
  }

  code {
    background: color-mix(in srgb, var(--fg) 15%, var(--bg));
    padding: 0.15rem 0.2rem;
    border-radius: 4px;
    font-size: 0.85em;
    color: var(--fg-heading);
  }

  /* This won't affect code inside .code-view, .inline-editor, etc. */
}
```

### Phase 5: Rule Context Panel

```css
@scope (.rule-context) {
  :scope {
    border: 1px solid var(--border);
    border-radius: 6px;
    margin: 0.5rem;
    background: var(--bg);
  }

  .header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.5rem 0.75rem;
    background: var(--accent-dim);
    border-radius: 5px 5px 0 0;
    border-bottom: 1px solid var(--border);
  }

  .id {
    font-size: 0.85rem;
    font-weight: 500;
    color: var(--accent);
  }

  .close {
    background: none;
    border: none;
    cursor: pointer;
    color: var(--fg-muted);
    padding: 0.25rem;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 4px;

    &:hover {
      background: var(--hover);
      color: var(--fg);
    }
  }

  .body { padding: 0.75rem; }

  .text {
    font-size: 0.85rem;
    color: var(--fg-muted);
    margin-bottom: 0.75rem;
    line-height: 1.5;
  }

  .refs {
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
  }

  .ref {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.35rem 0.5rem;
    border-radius: 4px;
    font-size: 0.8rem;
    cursor: pointer;
    text-decoration: none;
    color: var(--fg);

    &:hover { background: var(--hover); }
    &.active { background: var(--accent-dim); }
  }

  .back {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    margin-top: 0.75rem;
    padding: 0.35rem 0.5rem;
    border-radius: 4px;
    font-size: 0.8rem;
    cursor: pointer;
    color: var(--fg-muted);
    text-decoration: none;

    &:hover {
      background: var(--hover);
      color: var(--fg);
    }
  }
}
```

## Benefits of @scope

### 1. Simpler Class Names
- `.header` instead of `.modal-header`, `.inline-editor-header`
- More readable, less typing
- Natural component structure

### 2. No Accidental Leakage
- Styles truly contained within scope
- No need for BEM naming conventions
- Lower specificity concerns

### 3. Donut Scopes with `to`
- Perfect for markdown/rich content
- Styles stop at nested component boundaries
- Prevents style bleed into embeds

### 4. `:scope` Selector
- Style the scoping element itself
- Clean alternative to `.component { ... }`

## Browser Support

As of 2024, `@scope` is supported in:
- Chrome 118+ (October 2023)
- Edge 118+
- Safari 17.4+ (March 2024)
- Firefox: Behind flag (expected 2024)

**Fallback Strategy:**
```css
/* Fallback for older browsers */
.modal .header { ... }

/* Modern browsers */
@supports (selector(:scope)) {
  @scope (.modal) {
    .header { ... }
  }
}
```

## HTML Structure Changes

Simplify class names in markup:

**Before:**
```html
<div class="modal-overlay">
  <div class="modal-content editor-modal">
    <div class="modal-header">
      <h3>Edit</h3>
      <span class="modal-vim-indicator">VIM</span>
      <button class="modal-close">×</button>
    </div>
    <div class="modal-body">...</div>
    <div class="modal-footer">
      <div class="modal-info">...</div>
      <div class="modal-actions">
        <button class="modal-btn modal-btn-cancel">Cancel</button>
        <button class="modal-btn modal-btn-save">Save</button>
      </div>
    </div>
  </div>
</div>
```

**After:**
```html
<div class="modal">
  <div class="content editor">
    <div class="header">
      <h3>Edit</h3>
      <span class="vim-indicator">VIM</span>
      <button class="close-btn">×</button>
    </div>
    <div class="body">...</div>
    <div class="footer">
      <div class="info">...</div>
      <div class="actions">
        <button class="btn cancel">Cancel</button>
        <button class="btn save">Save</button>
      </div>
    </div>
  </div>
</div>
```

## Implementation Checklist

- [ ] Identify isolated component boundaries
- [ ] Refactor modal components
- [ ] Refactor search overlay
- [ ] Refactor inline editor
- [ ] Add donut scopes for markdown content
- [ ] Refactor rule context panel
- [ ] Refactor dropdown components
- [ ] Update HTML to use simpler class names
- [ ] Add fallback styles for Firefox
- [ ] Test in all target browsers

## Estimated Impact

- **Lines removed:** ~200 (prefix repetition)
- **Clarity gained:** Component boundaries explicit in CSS
- **Specificity:** Naturally lower, easier overrides
- **Maintenance:** Changes to component stay localized