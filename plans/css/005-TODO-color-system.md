# Plan 005: Color System Consolidation

## Overview

Consolidate the color mixing patterns and status colors into a more systematic approach using CSS custom property composition.

## Current State

Colors are mixed inline throughout the CSS:

```css
/* Scattered color-mix calls */
.req-container.req-uncovered {
    background: color-mix(in srgb, var(--accent) 5%, var(--bg));
}

.code-line.selected-impl {
    background: color-mix(in srgb, var(--green) 15%, transparent);
}

.req-badge.req-impl {
    background: color-mix(in srgb, var(--green) 15%, transparent);
    border: 1px solid color-mix(in srgb, var(--green) 30%, transparent);
}
```

Status colors are defined but inconsistently applied:

```css
--status-covered-bg: color-mix(in srgb, var(--green) 15%, var(--bg));
--status-covered-fg: var(--fg);
--status-partial-bg: var(--yellow-dim);
/* etc. */
```

## Problems

1. **Repeated `color-mix()` calls** - Same percentages used in many places
2. **Inconsistent alpha levels** - 5%, 10%, 12%, 15%, 20%, 25%, 30% used arbitrarily
3. **Mixed approaches** - Some use `transparent`, some use `var(--bg)`
4. **Hard to theme** - Changing a color requires updating many places
5. **No semantic naming** - `color-mix(in srgb, var(--green) 15%, transparent)` doesn't tell you it's for "implementation highlight"

## Proposed Changes

### Phase 1: Define Opacity Scales

Create standardized opacity levels for each semantic color:

```css
:root {
    /* Base colors (already exist) */
    --green: var(--arb-da);
    --red: var(--arb-dd);
    --yellow: var(--arb-n);
    --accent: var(--arb-at);
    
    /* Standardized opacity scale (5, 10, 15, 20, 30, 50) */
    --green-5: color-mix(in srgb, var(--green) 5%, var(--bg));
    --green-10: color-mix(in srgb, var(--green) 10%, var(--bg));
    --green-15: color-mix(in srgb, var(--green) 15%, var(--bg));
    --green-20: color-mix(in srgb, var(--green) 20%, var(--bg));
    --green-30: color-mix(in srgb, var(--green) 30%, var(--bg));
    
    --accent-5: color-mix(in srgb, var(--accent) 5%, var(--bg));
    --accent-10: color-mix(in srgb, var(--accent) 10%, var(--bg));
    --accent-15: color-mix(in srgb, var(--accent) 15%, var(--bg));
    --accent-20: color-mix(in srgb, var(--accent) 20%, var(--bg));
    --accent-30: color-mix(in srgb, var(--accent) 30%, var(--bg));
    
    --yellow-5: color-mix(in srgb, var(--yellow) 5%, var(--bg));
    --yellow-10: color-mix(in srgb, var(--yellow) 10%, var(--bg));
    --yellow-15: color-mix(in srgb, var(--yellow) 15%, var(--bg));
    --yellow-20: color-mix(in srgb, var(--yellow) 20%, var(--bg));
    --yellow-30: color-mix(in srgb, var(--yellow) 30%, var(--bg));
    
    --red-5: color-mix(in srgb, var(--red) 5%, var(--bg));
    --red-10: color-mix(in srgb, var(--red) 10%, var(--bg));
    --red-15: color-mix(in srgb, var(--red) 15%, var(--bg));
    --red-20: color-mix(in srgb, var(--red) 20%, var(--bg));
    --red-30: color-mix(in srgb, var(--red) 30%, var(--bg));
    
    --neutral-5: color-mix(in srgb, var(--neutral) 5%, var(--bg));
    --neutral-10: color-mix(in srgb, var(--neutral) 10%, var(--bg));
    --neutral-15: color-mix(in srgb, var(--neutral) 15%, var(--bg));
    --neutral-20: color-mix(in srgb, var(--neutral) 20%, var(--bg));
    --neutral-25: color-mix(in srgb, var(--neutral) 25%, var(--bg));
}
```

### Phase 2: Semantic Color Aliases

Map semantic meanings to the opacity scale:

```css
:root {
    /* Dim variants (for backgrounds) - replace existing *-dim vars */
    --accent-dim: var(--accent-20);
    --green-dim: var(--green-20);
    --yellow-dim: var(--yellow-20);
    --red-dim: var(--red-20);
    --neutral-dim: var(--neutral-25);
    
    /* Status backgrounds - consolidate existing status vars */
    --status-covered-bg: var(--green-15);
    --status-partial-bg: var(--yellow-20);
    --status-uncovered-bg: var(--accent);  /* Full color for emphasis */
    --status-none-bg: var(--neutral-dim);
    
    /* Selection/highlight backgrounds */
    --highlight-impl: var(--green-15);
    --highlight-verify: var(--accent-15);
    --highlight-search: var(--yellow-20);
    --highlight-selected: var(--accent-20);
    
    /* Border variants */
    --border-impl: var(--green-30);
    --border-verify: var(--accent-30);
    --border-partial: color-mix(in srgb, var(--yellow) 50%, var(--border));
}
```

### Phase 3: Reference Type System

Create a composable system for impl/verify/test styling:

```css
:root {
    /* Reference type colors */
    --ref-impl-color: var(--green);
    --ref-impl-bg: var(--green-15);
    --ref-impl-border: var(--green-30);
    
    --ref-verify-color: var(--accent);
    --ref-verify-bg: var(--accent-15);
    --ref-verify-border: var(--accent-30);
    
    --ref-test-color: var(--blue);
    --ref-test-bg: color-mix(in srgb, var(--blue) 15%, var(--bg));
    --ref-test-border: color-mix(in srgb, var(--blue) 30%, var(--bg));
}

/* Generic ref styling that uses the variables */
[data-ref-type="impl"] {
    --ref-color: var(--ref-impl-color);
    --ref-bg: var(--ref-impl-bg);
    --ref-border: var(--ref-impl-border);
}

[data-ref-type="verify"] {
    --ref-color: var(--ref-verify-color);
    --ref-bg: var(--ref-verify-bg);
    --ref-border: var(--ref-verify-border);
}

/* Components use the contextual variables */
.ref-icon { color: var(--ref-color, var(--fg-muted)); }
.ref-badge {
    background: var(--ref-bg);
    color: var(--ref-color);
    border: 1px solid var(--ref-border);
}
```

### Phase 4: Interactive State Colors

Standardize hover, active, focus states:

```css
:root {
    /* Interactive backgrounds */
    --interactive-hover: var(--hover);
    --interactive-active: var(--accent-10);
    --interactive-focus-ring: var(--accent-30);
    
    /* For components that need accent-colored interaction */
    --interactive-accent-hover: color-mix(in srgb, var(--accent) 90%, transparent);
}
```

## Migration Examples

### Before and After: Code Line Selection

**Before:**
```css
.code-line.selected {
    background: var(--accent-dim);
}

.code-line.selected-impl {
    background: color-mix(in srgb, var(--green) 15%, transparent);
}

.code-line.selected-verify {
    background: color-mix(in srgb, var(--accent) 15%, transparent);
}
```

**After:**
```css
.code-line.selected {
    background: var(--highlight-selected);
}

.code-line.selected-impl {
    background: var(--highlight-impl);
}

.code-line.selected-verify {
    background: var(--highlight-verify);
}
```

### Before and After: Badge Styling

**Before:**
```css
.req-badge.req-impl {
    background: color-mix(in srgb, var(--green) 15%, transparent);
    color: var(--green);
    border: 1px solid color-mix(in srgb, var(--green) 30%, transparent);
}

.req-badge.req-test {
    background: color-mix(in srgb, var(--accent) 15%, transparent);
    color: var(--accent);
    border: 1px solid color-mix(in srgb, var(--accent) 30%, transparent);
}
```

**After:**
```css
.req-badge.req-impl {
    background: var(--ref-impl-bg);
    color: var(--ref-impl-color);
    border: 1px solid var(--ref-impl-border);
}

.req-badge.req-test {
    background: var(--ref-verify-bg);
    color: var(--ref-verify-color);
    border: 1px solid var(--ref-verify-border);
}
```

### Before and After: Status Badges

**Before:**
```css
.folder-badge.full {
    background: var(--status-covered-bg);
    color: var(--status-covered-fg);
}
.folder-badge.partial {
    background: var(--status-partial-bg);
    color: var(--status-partial-fg);
}
.folder-badge.none {
    background: var(--status-none-bg);
    color: var(--status-none-fg);
}
```

**After (with data attributes from Plan 003):**
```css
.badge {
    &[data-status="covered"] {
        background: var(--status-covered-bg);
        color: var(--status-covered-fg);
    }
    &[data-status="partial"] {
        background: var(--status-partial-bg);
        color: var(--status-partial-fg);
    }
    &[data-status="none"] {
        background: var(--status-none-bg);
        color: var(--status-none-fg);
    }
}
```

## File Structure

After consolidation, the `:root` block should be organized:

```css
:root {
    /* === Color Scheme === */
    color-scheme: light dark;
    
    /* === Base Palette (from Arborium) === */
    --arb-at: ...;
    /* ... other arb colors ... */
    
    /* === Semantic Base Colors === */
    --accent: var(--arb-at);
    --green: var(--arb-da);
    --red: var(--arb-dd);
    --yellow: var(--arb-n);
    --blue: var(--arb-tu);
    --purple: var(--arb-s);
    --neutral: light-dark(#6b7ba8, #8b9bc8);
    
    /* === Color Opacity Scales === */
    /* (generated for each semantic color) */
    
    /* === Background Colors === */
    --bg-outer: ...;
    --bg: ...;
    --bg-secondary: ...;
    --bg-sidebar: ...;
    
    /* === Text Colors === */
    --fg-heading: ...;
    --fg: ...;
    --fg-muted: ...;
    --fg-dim: ...;
    
    /* === Border Colors === */
    --border: ...;
    --border-strong: ...;
    
    /* === Interactive States === */
    --hover: ...;
    --hover-subtle: ...;
    
    /* === Status Colors === */
    --status-covered-bg: ...;
    --status-covered-fg: ...;
    /* ... */
    
    /* === Reference Type Colors === */
    --ref-impl-color: ...;
    --ref-impl-bg: ...;
    /* ... */
    
    /* === Highlight Colors === */
    --highlight-impl: ...;
    --highlight-verify: ...;
    /* ... */
    
    /* === Layout === */
    --max-width: 1600px;
    
    /* === Typography === */
    --font-sans: ...;
    --font-mono: ...;
}
```

## Implementation Steps

1. **Add opacity scale variables** to `:root` without changing anything else
2. **Update semantic aliases** (`--accent-dim`, etc.) to use the scale
3. **Add new semantic variables** for highlights, refs, borders
4. **Search and replace** inline `color-mix()` calls with variables
5. **Verify visual consistency** - colors should look identical
6. **Remove unused variables** after migration is complete

## Benefits

1. **Single source of truth** - Change `--green` and all green variants update
2. **Consistent opacity levels** - No more arbitrary percentages
3. **Semantic naming** - `--highlight-impl` is clearer than `color-mix(...)`
4. **Easier theming** - Override semantic variables for different themes
5. **Smaller file size** - Variables compress better than repeated `color-mix()`
6. **Better maintainability** - Designers can adjust the scale in one place

## Dependencies

- None - can be implemented independently
- Pairs well with Plan 003 (Data Attributes) for status styling
- Supports Plan 001 (Nesting) by simplifying nested color rules