# Plan 012: Spacing System

## Overview

Replace ad-hoc spacing values with a consistent spacing scale using CSS custom properties. This brings visual rhythm and makes spacing adjustments global.

## Current State

Spacing values are scattered throughout with no consistent scale:

```css
/* Current: arbitrary values everywhere */
padding: 0.35rem 0.5rem;
padding: 0.4rem 0.6rem;
padding: 0.5rem 0.75rem;
padding: 0.75rem 1rem;
padding: 0.75rem 1.25rem;
gap: 0.25rem;
gap: 0.35rem;
gap: 0.5rem;
margin: 1rem 0;
margin: 1.5rem 0;
margin: 2rem 0;
```

## Proposed Spacing Scale

Based on a 4px (0.25rem) base unit:

```css
:root {
    /* Spacing scale (4px base) */
    --space-0: 0;
    --space-1: 0.25rem;   /* 4px - tight gaps */
    --space-2: 0.5rem;    /* 8px - small gaps, tight padding */
    --space-3: 0.75rem;   /* 12px - medium gaps */
    --space-4: 1rem;      /* 16px - standard padding */
    --space-5: 1.25rem;   /* 20px - comfortable padding */
    --space-6: 1.5rem;    /* 24px - section gaps */
    --space-8: 2rem;      /* 32px - large section gaps */
    --space-10: 2.5rem;   /* 40px - major sections */
    --space-12: 3rem;     /* 48px - hero spacing */
    
    /* Semantic spacing aliases */
    --space-xs: var(--space-1);
    --space-sm: var(--space-2);
    --space-md: var(--space-4);
    --space-lg: var(--space-6);
    --space-xl: var(--space-8);
    
    /* Component-specific spacing */
    --padding-btn: var(--space-2) var(--space-3);
    --padding-btn-sm: var(--space-1) var(--space-2);
    --padding-input: var(--space-2) var(--space-3);
    --padding-card: var(--space-4);
    --padding-section: var(--space-5);
    
    --gap-tight: var(--space-1);
    --gap-normal: var(--space-2);
    --gap-loose: var(--space-3);
    --gap-section: var(--space-4);
}
```

## Migration Examples

### Buttons

```css
/* Before */
.btn-md {
    padding: 0.5rem 1rem;
}
.btn-sm {
    padding: 0.375rem 0.75rem;
}

/* After */
.btn-md {
    padding: var(--space-2) var(--space-4);
}
.btn-sm {
    padding: var(--space-1) var(--space-3);
}

/* Or with semantic tokens */
.btn-md {
    padding: var(--padding-btn);
}
.btn-sm {
    padding: var(--padding-btn-sm);
}
```

### Flex Gaps

```css
/* Before */
.header-pickers {
    gap: 0.5rem;
}
.nav {
    gap: 0.25rem;
}
.stats-bar {
    gap: 2rem;
}

/* After */
.header-pickers {
    gap: var(--gap-normal);
}
.nav {
    gap: var(--gap-tight);
}
.stats-bar {
    gap: var(--space-8);
}
```

### Margins

```css
/* Before */
.markdown h1,
.markdown h2 {
    margin-top: 2.5rem;
    margin-bottom: 0.75rem;
}
.markdown p {
    margin: 1rem 0;
}

/* After */
.markdown h1,
.markdown h2 {
    margin-block-start: var(--space-10);
    margin-block-end: var(--space-3);
}
.markdown p {
    margin-block: var(--space-4);
}
```

### Padding

```css
/* Before */
.sidebar-header {
    padding: 0.75rem 1rem;
}
.content-header {
    padding: 0.5rem 1rem;
}
.markdown {
    padding: 0.75rem 2rem 1.5rem 2rem;
}

/* After */
.sidebar-header {
    padding: var(--space-3) var(--space-4);
}
.content-header {
    padding: var(--space-2) var(--space-4);
}
.markdown {
    padding: var(--space-3) var(--space-8) var(--space-6);
}
```

## Rounding Strategy

When migrating existing values, round to the nearest scale value:

| Current Value | Nearest Scale | Variable |
|---------------|---------------|----------|
| 0.1rem        | 0.25rem       | --space-1 |
| 0.15rem       | 0.25rem       | --space-1 |
| 0.25rem       | 0.25rem       | --space-1 |
| 0.3rem        | 0.25rem       | --space-1 |
| 0.35rem       | 0.5rem        | --space-2 |
| 0.4rem        | 0.5rem        | --space-2 |
| 0.5rem        | 0.5rem        | --space-2 |
| 0.6rem        | 0.5rem        | --space-2 |
| 0.75rem       | 0.75rem       | --space-3 |
| 0.8rem        | 0.75rem       | --space-3 |
| 1rem          | 1rem          | --space-4 |
| 1.25rem       | 1.25rem       | --space-5 |
| 1.5rem        | 1.5rem        | --space-6 |
| 2rem          | 2rem          | --space-8 |
| 2.5rem        | 2.5rem        | --space-10 |

## Spacing Utility Classes

For quick adjustments without custom CSS:

```css
/* Margin utilities */
.m-0 { margin: var(--space-0); }
.m-1 { margin: var(--space-1); }
.m-2 { margin: var(--space-2); }
.m-4 { margin: var(--space-4); }

.mt-0 { margin-block-start: var(--space-0); }
.mt-2 { margin-block-start: var(--space-2); }
.mt-4 { margin-block-start: var(--space-4); }

.mb-0 { margin-block-end: var(--space-0); }
.mb-2 { margin-block-end: var(--space-2); }
.mb-4 { margin-block-end: var(--space-4); }

/* Padding utilities */
.p-0 { padding: var(--space-0); }
.p-2 { padding: var(--space-2); }
.p-4 { padding: var(--space-4); }

/* Gap utilities */
.gap-1 { gap: var(--space-1); }
.gap-2 { gap: var(--space-2); }
.gap-3 { gap: var(--space-3); }
.gap-4 { gap: var(--space-4); }
```

## Component Spacing Patterns

### Cards/Panels

```css
.panel {
    padding: var(--padding-card);
    
    &-header {
        padding: var(--space-3) var(--space-4);
        margin: calc(-1 * var(--padding-card));
        margin-bottom: var(--space-4);
    }
    
    &-footer {
        padding: var(--space-3) var(--space-4);
        margin: calc(-1 * var(--padding-card));
        margin-top: var(--space-4);
    }
}
```

### Lists

```css
.list {
    --list-gap: var(--space-2);
    display: flex;
    flex-direction: column;
    gap: var(--list-gap);
    
    &-tight { --list-gap: var(--space-1); }
    &-loose { --list-gap: var(--space-3); }
}
```

### Form Elements

```css
.form-group {
    margin-bottom: var(--space-4);
}

.form-label {
    margin-bottom: var(--space-1);
}

.form-input {
    padding: var(--padding-input);
}

.form-actions {
    margin-top: var(--space-6);
    gap: var(--space-3);
}
```

## Responsive Spacing

Scale can adjust at breakpoints:

```css
:root {
    --space-unit: 0.25rem;
}

@media (min-width: 768px) {
    :root {
        --space-unit: 0.25rem; /* Same, but could increase */
    }
}

/* Dynamic spacing based on unit */
:root {
    --space-1: calc(1 * var(--space-unit));
    --space-2: calc(2 * var(--space-unit));
    --space-3: calc(3 * var(--space-unit));
    --space-4: calc(4 * var(--space-unit));
    /* etc. */
}
```

## Implementation Approach

### Phase 1: Add Variables

Add the spacing scale to `:root` without changing existing code.

### Phase 2: High-Impact Components

Migrate major components first:
- Buttons
- Cards/panels
- Headers
- Sidebar

### Phase 3: Typography

Update markdown and content spacing.

### Phase 4: Fine Details

Migrate badges, icons, and small details.

### Phase 5: Add Utilities

Add utility classes for common adjustments.

## Files Affected

- Variables section at top of `style.css`
- All components with padding/margin/gap

## Visual Audit Points

After migration, verify:
- [ ] Buttons feel consistent across sizes
- [ ] Card/panel padding is uniform
- [ ] List item spacing is even
- [ ] Headers have appropriate breathing room
- [ ] Form elements align on the grid
- [ ] No awkward gaps or cramped areas

## Related Plans

- **Plan 002**: Utility classes for spacing
- **Plan 011**: Typography (line-height interacts with spacing)
- **Plan 006**: Modal spacing
- **Plan 003**: Badge padding

## Notes

- Some intentional "off-grid" values may exist for optical alignment
- Border widths can affect perceived spacing
- Test with different content lengths
- Consider `gap` as primary spacing mechanism for flex/grid layouts