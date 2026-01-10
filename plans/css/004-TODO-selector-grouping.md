# Plan 004: Selector Grouping with `:is()` and `:where()`

## Overview

CSS now has powerful pseudo-class functions for grouping selectors: `:is()` and `:where()`. These reduce repetition and make stylesheets more maintainable.

## Key Differences

- **`:is()`** - Takes the specificity of its most specific argument
- **`:where()`** - Always has zero specificity (great for defaults that can be overridden)

## Current Patterns to Refactor

### 1. Repeated Hover States

**Before:**
```css
.tree-file:hover {
    background: var(--hover);
}

.tree-folder-header:hover {
    background: var(--hover);
}

.toc-row:hover {
    background: var(--hover);
}

.dropdown-option:hover {
    background: var(--hover);
}

.search-result:hover {
    background: var(--hover);
}
```

**After:**
```css
:is(.tree-file, .tree-folder-header, .toc-row, .dropdown-option, .search-result):hover {
    background: var(--hover);
}
```

### 2. Markdown Element Styling

**Before:**
```css
.markdown h1,
.markdown h2,
.markdown h3,
.markdown h4 {
    margin: 1.5rem 0 0.75rem;
    color: var(--fg-heading);
    cursor: pointer;
}

.markdown h1:hover,
.markdown h2:hover,
.markdown h3:hover,
.markdown h4:hover {
    color: var(--accent);
}

.markdown h1:target,
.markdown h2:target,
.markdown h3:target,
.markdown h4:target {
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 10%, transparent);
    /* ... */
}
```

**After:**
```css
.markdown :is(h1, h2, h3, h4) {
    margin: 1.5rem 0 0.75rem;
    color: var(--fg-heading);
    cursor: pointer;
    
    &:hover {
        color: var(--accent);
    }
    
    &:target {
        color: var(--accent);
        background: color-mix(in srgb, var(--accent) 10%, transparent);
        /* ... */
    }
}
```

### 3. RFC 2119 Keywords

**Before:**
```css
kw-must,
kw-must-not,
kw-required,
kw-shall,
kw-shall-not {
    color: var(--red);
    font-weight: 600;
}

kw-should,
kw-should-not,
kw-recommended,
kw-not-recommended {
    color: var(--yellow);
    font-weight: 600;
}

kw-may,
kw-optional {
    color: var(--accent);
    font-weight: 600;
}
```

**After:**
```css
:is(kw-must, kw-must-not, kw-required, kw-shall, kw-shall-not, kw-should, kw-should-not, kw-recommended, kw-not-recommended, kw-may, kw-optional) {
    font-weight: 600;
}

:is(kw-must, kw-must-not, kw-required, kw-shall, kw-shall-not) {
    color: var(--red);
}

:is(kw-should, kw-should-not, kw-recommended, kw-not-recommended) {
    color: var(--yellow);
}

:is(kw-may, kw-optional) {
    color: var(--accent);
}
```

### 4. Paragraph Edit Button Containers

**Before:**
```css
.markdown p,
.markdown li,
.markdown blockquote {
    position: relative;
}

.markdown p:hover > .para-edit-btn,
.markdown li:hover > .para-edit-btn,
.markdown blockquote:hover > .para-edit-btn {
    opacity: 1;
}
```

**After:**
```css
.markdown :is(p, li, blockquote) {
    position: relative;
    
    &:hover > .para-edit-btn {
        opacity: 1;
    }
}
```

### 5. Code/Mono Elements

**Before:**
```css
.mono,
code,
pre {
    font-family: var(--font-mono);
    font-variation-settings: "MONO" 1, "CASL" 0;
}
```

**After:**
```css
:is(.mono, code, pre) {
    font-family: var(--font-mono);
    font-variation-settings: "MONO" 1, "CASL" 0;
}
```

### 6. Reference Type Icons

**Before:**
```css
.spec-ref-icon-impl,
.ref-icon-impl,
.file-path-icon-impl {
    color: var(--ref-impl-color);
}

.spec-ref-icon-verify,
.ref-icon-verify,
.file-path-icon-verify {
    color: var(--ref-verify-color);
}
```

**After:**
```css
:is(.spec-ref-icon, .ref-icon, .file-path-icon) {
    &-impl, &[data-type="impl"] {
        color: var(--ref-impl-color);
    }
    
    &-verify, &[data-type="verify"] {
        color: var(--ref-verify-color);
    }
}
```

### 7. Using `:where()` for Low-Specificity Defaults

**Before (base styles that need to be overridable):**
```css
.markdown ul,
.markdown ol {
    margin: 0.75rem 0;
    padding-left: 1.5rem;
}

.markdown li {
    margin: 0.25rem 0;
}
```

**After:**
```css
/* Zero specificity - easy to override in components */
:where(.markdown) :is(ul, ol) {
    margin: 0.75rem 0;
    padding-left: 1.5rem;
}

:where(.markdown) li {
    margin: 0.25rem 0;
}

/* Component-specific override wins without !important */
.req-content :is(ul, ol) {
    margin: 0.75rem 0;
    padding-left: 0.75rem;
}
```

### 8. Modal Variants

**Before:**
```css
.modal-loading,
.modal-error {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--fg-muted);
    font-size: 0.9rem;
}

.modal-error {
    color: var(--red);
}

.inline-editor-loading,
.inline-editor-error {
    padding: 1rem;
    text-align: center;
    color: var(--fg-muted);
    font-size: 0.85rem;
}

.inline-editor-error {
    color: var(--red);
}
```

**After:**
```css
:is(.modal, .inline-editor)-loading,
:is(.modal, .inline-editor)-error {
    color: var(--fg-muted);
}

:is(.modal, .inline-editor)-error {
    color: var(--red);
}

.modal-loading,
.modal-error {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 0.9rem;
}

.inline-editor-loading,
.inline-editor-error {
    padding: 1rem;
    text-align: center;
    font-size: 0.85rem;
}
```

## Complex Grouping Patterns

### Combining with Nesting

```css
.btn {
    /* base styles */
    
    &:is(:hover, :focus-visible) {
        /* hover and focus states */
    }
    
    &:is(.btn-primary, .btn-accent) {
        background: var(--accent);
        
        &:hover {
            filter: brightness(1.1);
        }
    }
    
    &:is(:disabled, [aria-disabled="true"]) {
        opacity: 0.5;
        cursor: not-allowed;
    }
}
```

### Negation Patterns

```css
/* All buttons except disabled */
.btn:not(:is(:disabled, [aria-disabled="true"])):hover {
    filter: brightness(1.1);
}

/* All interactive elements that aren't links */
:is(button, [role="button"]):not(a) {
    cursor: pointer;
}
```

### Forgiving Selector Lists

`:is()` and `:where()` are forgiving - if one selector is invalid, others still work:

```css
/* If :has() isn't supported, the whole rule still applies */
:is(.sidebar:has(.active), .sidebar.has-active) {
    border-color: var(--accent);
}
```

## Implementation Steps

1. **Identify repeated selectors** - Search for patterns like `X, Y, Z { same-styles }`
2. **Group hover/focus states** - These are the most common candidates
3. **Use `:where()` for base styles** - Keep specificity low for component defaults
4. **Combine with nesting** - `:is()` inside nested rules is very powerful
5. **Test specificity** - Ensure overrides still work as expected

## Specificity Considerations

| Selector | Specificity |
|----------|-------------|
| `:is(.a, .b, #c)` | 1,0,0 (highest in list) |
| `:where(.a, .b, #c)` | 0,0,0 (always zero) |
| `.a, .b, #c` | Each keeps its own |

## Browser Support

- `:is()` - 95%+ (all modern browsers)
- `:where()` - 95%+ (all modern browsers)
- Forgiving selector parsing - Part of the spec

## Files to Modify

1. `style.css` - Apply grouping throughout
2. Focus on sections with many comma-separated selectors
3. Use `:where()` for base/reset styles that components might override

## Expected Results

- Reduced selector repetition
- Clearer intent (grouping shows related elements)
- Better control over specificity with `:where()`
- Easier maintenance when adding new similar elements