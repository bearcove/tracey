# Plan 002: Utility Classes

## Overview

Extract common patterns into reusable utility classes to reduce duplication and improve consistency. This is NOT about going full Tailwind—it's about identifying the most repeated patterns and giving them names.

## Motivation

The current CSS has these patterns repeated 20+ times each:

- `display: flex; align-items: center;` — ~50 occurrences
- `display: flex; align-items: center; gap: 0.5rem;` — ~30 occurrences
- `cursor: pointer; transition: ... :hover { background: var(--hover); }` — ~40 occurrences
- `font-size: 0.85rem;` — ~25 occurrences
- `border-radius: 4px/6px;` — ~60 occurrences

## Proposed Utilities

### Layout Utilities

```css
/* Flex containers */
.flex { display: flex; }
.flex-col { display: flex; flex-direction: column; }
.flex-1 { flex: 1; }
.flex-shrink-0 { flex-shrink: 0; }

/* Alignment */
.items-center { align-items: center; }
.items-start { align-items: flex-start; }
.justify-center { justify-content: center; }
.justify-between { justify-content: space-between; }

/* Gap scale (matches spacing scale) */
.gap-1 { gap: 0.25rem; }
.gap-2 { gap: 0.5rem; }
.gap-3 { gap: 0.75rem; }
.gap-4 { gap: 1rem; }
```

### Interactive Utilities

```css
/* Base interactive element - adds pointer + smooth transitions */
.interactive {
    cursor: pointer;
    transition: background 0.15s, color 0.15s, opacity 0.15s;
}

/* Hover backgrounds */
.hover\:bg:hover { background: var(--hover); }
.hover\:bg-subtle:hover { background: var(--hover-subtle); }

/* Combined: interactive + hover effect */
.clickable {
    cursor: pointer;
    transition: background 0.15s;
}
.clickable:hover {
    background: var(--hover);
}
```

### Typography Utilities

```css
/* Font sizes - semantic scale */
.text-xs { font-size: 0.7rem; }
.text-sm { font-size: 0.8rem; }
.text-base { font-size: 0.85rem; }
.text-md { font-size: 0.9rem; }
.text-lg { font-size: 1rem; }

/* Font weights */
.font-medium { font-weight: 500; }
.font-semibold { font-weight: 600; }

/* Colors */
.text-muted { color: var(--fg-muted); }
.text-dim { color: var(--fg-dim); }
.text-accent { color: var(--accent); }
```

### Visual Utilities

```css
/* Border radius scale */
.rounded-sm { border-radius: 3px; }
.rounded { border-radius: 4px; }
.rounded-md { border-radius: 6px; }
.rounded-lg { border-radius: 8px; }
.rounded-full { border-radius: 9999px; }

/* Overflow */
.overflow-hidden { overflow: hidden; }
.overflow-auto { overflow: auto; }
.truncate {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
}
```

## Implementation Strategy

### Phase 1: Define Utilities

Add a new section at the top of `style.css` (after `:root`):

```css
/* ==========================================================================
   UTILITIES
   Reusable single-purpose classes. Use sparingly in component CSS,
   more freely in markup for layout adjustments.
   ========================================================================== */

/* Layout */
.flex { display: flex; }
/* ... etc ... */
```

### Phase 2: Refactor Components (Selective)

NOT every component needs to use utilities. Use them when:

1. **The pattern is truly generic** (flex centering, gaps)
2. **It reduces a 4+ line declaration to 1** 
3. **It's applied in markup**, not nested CSS

**Good candidate:**
```css
/* Before */
.header-pickers {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0 1rem;
    border-right: 1px solid var(--border);
}

/* After - in CSS */
.header-pickers {
    padding: 0 1rem;
    border-right: 1px solid var(--border);
}

/* In markup: <div class="header-pickers flex items-center gap-2"> */
```

**Bad candidate (keep as-is):**
```css
/* This is component-specific styling, not a pattern to extract */
.rule-marker-highlighted {
    animation: rule-highlight-pulse 3s ease-out;
}
```

### Phase 3: Document Usage Guidelines

Add a comment block explaining when to use utilities vs component classes:

```css
/*
 * UTILITY CLASS GUIDELINES:
 *
 * ✅ Use utilities for:
 *    - Quick layout adjustments in markup
 *    - Centering, gaps, text alignment
 *    - One-off overrides
 *
 * ❌ Don't use utilities for:
 *    - Complex component styling
 *    - Pseudo-elements (:before, :after)
 *    - Media query variations
 *    - Hover/focus states (use .clickable or component styles)
 */
```

## Affected Patterns

| Pattern | Current Occurrences | Utility Name |
|---------|---------------------|--------------|
| `display: flex` | ~50 | `.flex` |
| `align-items: center` | ~45 | `.items-center` |
| `gap: 0.5rem` | ~30 | `.gap-2` |
| `cursor: pointer` | ~40 | `.clickable` |
| `font-size: 0.85rem` | ~25 | `.text-base` |
| `border-radius: 6px` | ~35 | `.rounded-md` |
| `overflow: hidden` | ~15 | `.overflow-hidden` |
| truncate pattern | ~10 | `.truncate` |

## File Changes

| File | Changes |
|------|---------|
| `style.css` | Add ~50 lines of utility definitions |
| Various components | Optional refactoring to use utilities |

## Considerations

1. **Don't over-extract**: Only extract patterns that appear 5+ times
2. **Keep specificity low**: Utilities should be easy to override
3. **Naming convention**: Follow Tailwind-like naming for familiarity
4. **Escape hatches**: Utilities in markup, specifics in CSS

## Testing

1. Visual regression: Screenshot key pages before/after
2. Verify no style changes when only adding utility definitions
3. Test that utilities can be overridden by component classes

## Dependencies

- None (can be done before or after nesting)
- Complements nesting by reducing what needs to be nested