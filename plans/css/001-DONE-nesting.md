# 001: Native CSS Nesting Migration

## Status: ✅ COMPLETED

**Completed:** 2025-01-10

### Summary
- Converted entire `style.css` to use native CSS nesting
- Added clear section headers with `/* ========== */` blocks
- Applied `:is()` selector grouping where appropriate (e.g., RFC 2119 keywords, markdown headings)
- File size: 2659 lines → 2725 lines (slight increase due to better formatting/comments)
- Character count: ~58k characters

### Key Changes
- All component styles now use `&` nesting syntax
- Hover/active/focus states nested within their parent selectors
- BEM-like modifiers (`&-header`, `&-content`) used consistently
- Media queries nested inside relevant rules
- Adjacent sibling rules (`& + &`) properly nested

---

## Overview

Native CSS nesting is now supported in all major browsers (Chrome 112+, Firefox 117+, Safari 16.5+). This feature alone can reduce our CSS file size by ~30-40% by eliminating repetitive parent selectors.

## Current State

We have many repeated selector patterns like:

```css
.tree-folder {
    user-select: none;
}

.tree-folder-header {
    display: flex;
    align-items: center;
    gap: 0.25rem;
    padding: 0.35rem 0.5rem;
}

.tree-folder-header:hover {
    background: var(--hover);
}

.tree-folder.open > .tree-folder-header .tree-folder-icon {
    transform: rotate(90deg);
}

.tree-folder-children {
    display: none;
    padding-left: 1rem;
}

.tree-folder.open > .tree-folder-children {
    display: block;
}
```

## Target State

Using native nesting:

```css
.tree-folder {
    user-select: none;

    &-header {
        display: flex;
        align-items: center;
        gap: 0.25rem;
        padding: 0.35rem 0.5rem;

        &:hover {
            background: var(--hover);
        }
    }

    &-icon {
        width: 1rem;
        height: 1rem;
        color: var(--fg-dim);
        transition: transform 0.15s;
    }

    &-children {
        display: none;
        padding-left: 1rem;
    }

    &.open {
        & > .tree-folder-header .tree-folder-icon {
            transform: rotate(90deg);
        }

        & > .tree-folder-children {
            display: block;
        }
    }
}
```

## Components to Refactor

### High-Impact (Many Nested Rules)

1. **File Tree** (`.tree-folder`, `.tree-file`, children)
2. **TOC/Outline** (`.toc-item`, `.toc-row`, `.toc-link`, `.toc-badges`, `.toc-children`)
3. **Requirements** (`.req-container`, `.req-badge`, `.req-content`, variants)
4. **Modals** (`.modal-overlay`, `.modal-content`, `.modal-header`, etc.)
5. **Search** (`.search-modal`, `.search-result`, variants)
6. **Code View** (`.code-line`, `.line-number`, `.line-content`, `.line-annotations`)
7. **Markdown** (`.markdown` with all nested element styles)
8. **Buttons** (`.btn`, `.btn-primary`, `.btn-secondary`, etc.)
9. **Rules Table** (`.rules-table`, `th`, `td`, `.rule-id`, etc.)
10. **Inline Editor** (`.inline-editor`, all sub-components)

### Medium-Impact

11. **Header** (`.header`, `.header-inner`, `.header-pickers`, etc.)
12. **Sidebar** (`.sidebar`, `.sidebar-header`, `.sidebar-content`)
13. **Stats Bar** (`.stats-bar`, `.stat`, `.stat-label`, `.stat-value`)
14. **Custom Dropdowns** (`.custom-dropdown`, `.dropdown-menu`, etc.)
15. **Rule Context Panel** (`.rule-context`, all children)
16. **Split View** (`.split-view`, `.split-pane`, etc.)

### Low-Impact (Few Rules)

17. **Layout** (`.layout`, `.main`, `.content`)
18. **Spec Switcher** (`.spec-switcher`, `.spec-tab`)
19. **Empty States** (`.empty-state`, `.loading`)

## Nesting Syntax Reference

### Basic Nesting

```css
.parent {
    color: blue;

    .child {
        color: red;
    }
}
/* Equivalent to: .parent .child { color: red; } */
```

### The `&` Selector

```css
.btn {
    background: blue;

    &:hover {
        background: lightblue;
    }

    &.active {
        background: darkblue;
    }

    &-icon {
        /* Creates .btn-icon */
        width: 1rem;
    }
}
```

### Compound Selectors (Require `&`)

```css
.item {
    & + & {
        margin-top: 1rem;
    }

    &.selected {
        background: var(--accent);
    }
}
```

### Media Queries Inside Rules

```css
.sidebar {
    width: 300px;

    @media (max-width: 768px) {
        width: 100%;
    }
}
```

## Migration Steps

### Step 1: Set Up Structure

Organize the file into logical sections with comments:

```css
/* ==========================================================================
   File Tree
   ========================================================================== */

.tree-folder {
    /* ... nested rules ... */
}

.tree-file {
    /* ... nested rules ... */
}
```

### Step 2: Migrate Component by Component

For each component:

1. Identify the root selector
2. Indent all related selectors under it
3. Replace parent references with `&`
4. Test in browser

### Step 3: Handle Edge Cases

Some patterns need careful handling:

```css
/* Before: Adjacent sibling */
.split-pane + .split-pane {
    border-left: 1px solid var(--border);
}

/* After: Must use & for adjacent */
.split-pane {
    & + & {
        border-left: 1px solid var(--border);
    }
}
```

```css
/* Before: Complex state selector */
.tree-folder.open > .tree-folder-header .tree-folder-icon {
    transform: rotate(90deg);
}

/* After: Nested with & */
.tree-folder {
    &.open > .tree-folder-header .tree-folder-icon {
        transform: rotate(90deg);
    }
}
```

## Verification Checklist

- [ ] All hover states work correctly
- [ ] All active/selected states work
- [ ] State modifiers (`.open`, `.active`, `.selected`) apply correctly
- [ ] Adjacent sibling rules (`+`) work
- [ ] Media queries inside nested blocks work
- [ ] No specificity regressions
- [ ] Dark mode still works (light-dark() in variables)

## Estimated Impact

| Metric | Before | After | Reduction |
|--------|--------|-------|-----------|
| Lines of CSS | ~2660 | ~1800 | ~32% |
| Selector repetition | High | Low | ~70% |
| Readability | Fair | Good | - |
| Maintainability | Fair | Good | - |

## Browser Support

- Chrome/Edge: 112+ (March 2023)
- Firefox: 117+ (August 2023)
- Safari: 16.5+ (May 2023)

For older browser support, consider running through a PostCSS plugin like `postcss-nesting` as a build step, though this is likely unnecessary for a developer tool.

## Dependencies

- None (pure CSS feature)

## Risks

- **Low**: Nesting is well-supported
- **Testing**: Need to verify all interactive states still work
- **Specificity**: Native nesting has the same specificity as non-nested equivalents

## Next Steps

After completing nesting migration, proceed to:
- 002: Utility Classes (to reduce remaining repetition)
- 003: Data Attribute Variants (to simplify badge/status classes)