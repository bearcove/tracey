# Plan 006: Logical Properties

## Overview

Migrate from physical CSS properties (`left`, `right`, `padding-left`, etc.) to logical properties (`inline-start`, `inline-end`, `padding-inline-start`, etc.) for better internationalization support and modern CSS practices.

## Why Logical Properties?

### Physical vs Logical

| Physical Property | Logical Property | Notes |
|-------------------|------------------|-------|
| `left` | `inset-inline-start` | Start of reading direction |
| `right` | `inset-inline-end` | End of reading direction |
| `top` | `inset-block-start` | Start of block flow |
| `bottom` | `inset-block-end` | End of block flow |
| `padding-left` | `padding-inline-start` | - |
| `padding-right` | `padding-inline-end` | - |
| `margin-left` | `margin-inline-start` | - |
| `margin-right` | `margin-inline-end` | - |
| `border-left` | `border-inline-start` | - |
| `border-right` | `border-inline-end` | - |
| `text-align: left` | `text-align: start` | - |
| `text-align: right` | `text-align: end` | - |
| `width` | `inline-size` | In horizontal writing mode |
| `height` | `block-size` | In horizontal writing mode |

### Benefits

1. **RTL Support**: Automatically mirrors for right-to-left languages
2. **Writing Mode Support**: Works correctly with vertical text
3. **Future-Proof**: The modern way to write CSS
4. **Semantic Clarity**: Describes intent rather than physical position

## Current Usage Analysis

### High-Priority Migrations

These patterns appear frequently and should be migrated first:

```css
/* Padding */
padding-left: 1rem;      → padding-inline-start: 1rem;
padding-right: 1rem;     → padding-inline-end: 1rem;

/* Margins */
margin-left: auto;       → margin-inline-start: auto;
margin-right: 0.5rem;    → margin-inline-end: 0.5rem;

/* Borders */
border-left: 3px solid;  → border-inline-start: 3px solid;
border-right: 1px solid; → border-inline-end: 1px solid;

/* Text alignment */
text-align: left;        → text-align: start;
text-align: right;       → text-align: end;

/* Positioning */
left: 0;                 → inset-inline-start: 0;
right: 1rem;             → inset-inline-end: 1rem;
```

### Shorthand Properties

Use new logical shorthands where available:

```css
/* Before */
padding-left: 1rem;
padding-right: 1rem;

/* After */
padding-inline: 1rem;

/* Before */
margin-top: 1rem;
margin-bottom: 1rem;

/* After */
margin-block: 1rem;

/* Before */
top: 0;
right: 0;
bottom: 0;
left: 0;

/* After */
inset: 0;

/* Or for specific axes */
inset-inline: 0;   /* left and right */
inset-block: 0;    /* top and bottom */
```

## Files to Update

### Line Numbers Component
```css
/* Before */
.line-number {
    width: 4rem;
    padding: 0 0.75rem 0 0.5rem;
    text-align: right;
    border-left: 3px solid transparent;
}

/* After */
.line-number {
    inline-size: 4rem;
    padding-inline: 0.5rem 0.75rem;
    text-align: end;
    border-inline-start: 3px solid transparent;
}
```

### Tree/Sidebar Components
```css
/* Before */
.tree-folder-children {
    padding-left: 1rem;
}

.toc-children {
    padding-left: 0.75rem;
    margin-left: 0.5rem;
    border-left: 1px solid var(--border-strong);
}

/* After */
.tree-folder-children {
    padding-inline-start: 1rem;
}

.toc-children {
    padding-inline-start: 0.75rem;
    margin-inline-start: 0.5rem;
    border-inline-start: 1px solid var(--border-strong);
}
```

### Markdown Content
```css
/* Before */
.markdown blockquote {
    border-left: 3px solid var(--accent);
}

.markdown ul,
.markdown ol {
    padding-left: 1.5rem;
}

/* After */
.markdown blockquote {
    border-inline-start: 3px solid var(--accent);
}

.markdown :is(ul, ol) {
    padding-inline-start: 1.5rem;
}
```

### Popovers and Positioned Elements
```css
/* Before */
.line-popover {
    position: absolute;
    left: 100%;
    margin-left: 0.5rem;
}

.para-edit-btn {
    position: absolute;
    right: -1.5rem;
}

/* After */
.line-popover {
    position: absolute;
    inset-inline-start: 100%;
    margin-inline-start: 0.5rem;
}

.para-edit-btn {
    position: absolute;
    inset-inline-end: -1.5rem;
}
```

### Requirement Containers
```css
/* Before */
.req-container::before {
    left: 0;
    border-radius: 0 2px 2px 0;
}

.req-badges-left {
    left: 0.75rem;
}

.req-badges-right {
    right: 1rem;
}

/* After */
.req-container::before {
    inset-inline-start: 0;
    border-start-start-radius: 0;
    border-start-end-radius: 2px;
    border-end-end-radius: 2px;
    border-end-start-radius: 0;
}

.req-badges-left {
    inset-inline-start: 0.75rem;
}

.req-badges-right {
    inset-inline-end: 1rem;
}
```

### Layout Components
```css
/* Before */
.sidebar {
    width: 360px;
    border-right: 1px solid var(--border);
}

.split-pane + .split-pane {
    border-left: 1px solid var(--border);
}

/* After */
.sidebar {
    inline-size: 360px;
    border-inline-end: 1px solid var(--border);
}

.split-pane + .split-pane {
    border-inline-start: 1px solid var(--border);
}
```

## Logical Border Radius

Border radius gets verbose with logical properties. Consider keeping physical values or using a variable:

```css
/* Physical (simpler, usually fine) */
border-radius: 0 4px 4px 0;

/* Logical (verbose but correct for RTL) */
border-start-start-radius: 0;
border-start-end-radius: 4px;
border-end-end-radius: 4px;
border-end-start-radius: 0;

/* Practical approach: variable */
:root {
    --radius-end: 0 4px 4px 0;
    --radius-start: 4px 0 0 4px;
}

/* Only use logical for critical RTL elements */
```

## Exceptions

Some properties should remain physical:

1. **Transform origins**: `transform-origin: left` describes the physical point
2. **Animations**: Physical movement (e.g., sliding from actual left side)
3. **Decorative elements**: Position doesn't change based on reading direction
4. **Scroll positions**: `scroll-left` is physical

## Implementation Strategy

### Phase 1: High-Impact Areas
- Sidebar and tree indentation
- Line numbers (code view)
- Markdown content lists/blockquotes

### Phase 2: Layout
- Split pane borders
- Header/footer layouts
- Modal positioning

### Phase 3: Components
- Badges and pills
- Popovers and tooltips
- Form elements

### Phase 4: Review
- Audit remaining physical properties
- Test with `direction: rtl` to verify

## Testing RTL

Add a simple test mode:

```css
/* Add to :root for testing */
html[dir="rtl"] {
    direction: rtl;
}

/* Or force it temporarily */
html {
    direction: rtl; /* Remove after testing */
}
```

Test these scenarios:
- Sidebar appears on the right
- Code line numbers appear on the right
- Tree indentation goes right-to-left
- Blockquote borders appear on the right

## Metrics

- **Physical properties to migrate**: ~80-100 occurrences
- **Browser support**: 95%+ (all modern browsers)
- **Risk**: Low (purely presentational change)

## Dependencies

- None (this is a pure CSS refactor)
- Can be done incrementally alongside other plans

## References

- [MDN: CSS Logical Properties](https://developer.mozilla.org/en-US/docs/Web/CSS/CSS_logical_properties_and_values)
- [CSS Tricks: CSS Logical Properties](https://css-tricks.com/css-logical-properties-and-values/)
- [Web.dev: Logical Properties](https://web.dev/learn/css/logical-properties/)