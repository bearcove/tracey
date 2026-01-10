# 003: Data Attributes for Variants

## Overview

Replace multiple CSS classes for state variants with data attributes. This reduces class proliferation and makes the relationship between base styles and variants explicit.

## Current Problem

The codebase has many patterns like:

```css
.folder-badge.full { background: var(--status-covered-bg); }
.folder-badge.partial { background: var(--status-partial-bg); }
.folder-badge.none { background: var(--status-none-bg); }

.tree-file-badge.full { /* same styles */ }
.tree-file-badge.partial { /* same styles */ }
.tree-file-badge.none { /* same styles */ }
```

This leads to:
- Duplicated variant definitions across similar components
- Class name collisions (`.full`, `.partial` are generic)
- Unclear relationship between variants

## Proposed Solution

Use data attributes for variants:

```css
/* Single definition for all status badges */
[data-status] {
    font-size: 0.7rem;
    padding: 0.1rem 0.4rem;
    border-radius: 4px;
    font-weight: 500;
}

[data-status="covered"] {
    background: var(--status-covered-bg);
    color: var(--status-covered-fg);
}

[data-status="partial"] {
    background: var(--status-partial-bg);
    color: var(--status-partial-fg);
}

[data-status="uncovered"] {
    background: var(--status-uncovered-bg);
    color: var(--status-uncovered-fg);
}

[data-status="none"] {
    background: var(--status-none-bg);
    color: var(--status-none-fg);
}
```

HTML changes:
```html
<!-- Before -->
<span class="folder-badge full">100%</span>
<span class="tree-file-badge partial">75%</span>

<!-- After -->
<span class="badge" data-status="covered">100%</span>
<span class="badge" data-status="partial">75%</span>
```

## Target Patterns

### 1. Status Badges (covered/partial/uncovered/none)

**Affected selectors:**
- `.folder-badge.full`, `.folder-badge.partial`, `.folder-badge.none`
- `.tree-file-badge.full`, `.tree-file-badge.partial`, `.tree-file-badge.none`
- `.rule-marker.covered`, `.rule-marker.partial`, `.rule-marker.uncovered`
- `.req-container.req-covered`, `.req-container.req-partial`, `.req-container.req-uncovered`

**Solution:**
```css
[data-status="covered"] { /* styles */ }
[data-status="partial"] { /* styles */ }
[data-status="uncovered"] { /* styles */ }
[data-status="none"] { /* styles */ }
```

### 2. Reference Types (impl/verify)

**Affected selectors:**
- `.spec-ref-icon-impl`, `.spec-ref-icon-verify`
- `.ref-icon-impl`, `.ref-icon-verify`
- `.file-path-icon-impl`, `.file-path-icon-verify`
- `.rule-ref.impl`, `.rule-ref.verify`

**Solution:**
```css
[data-ref-type="impl"] { color: var(--ref-impl-color); }
[data-ref-type="verify"] { color: var(--ref-verify-color); }
```

### 3. Stat Values (good/warn/bad)

**Affected selectors:**
- `.stat-value.good`, `.stat-value.warn`, `.stat-value.bad`

**Solution:**
```css
[data-quality="good"] { color: var(--green); }
[data-quality="warn"] { color: var(--yellow); }
[data-quality="bad"] { color: var(--red); }
```

### 4. Button Variants (primary/secondary/ghost)

**Affected selectors:**
- `.btn-primary`, `.btn-secondary`, `.btn-ghost`

**Solution:**
```css
.btn[data-variant="primary"] { background: var(--accent); color: white; }
.btn[data-variant="secondary"] { background: var(--bg-secondary); }
.btn[data-variant="ghost"] { background: transparent; }
```

### 5. Size Variants (sm/md)

**Affected selectors:**
- `.btn-sm`, `.btn-md`

**Solution:**
```css
.btn[data-size="sm"] { padding: 0.375rem 0.75rem; font-size: 0.8125rem; }
.btn[data-size="md"] { padding: 0.5rem 1rem; }
```

### 6. Level Dots (must/should/may)

**Affected selectors:**
- `.level-dot-all`, `.level-dot-must`, `.level-dot-should`, `.level-dot-may`

**Solution:**
```css
.level-dot[data-level="all"] { background: var(--fg-muted); }
.level-dot[data-level="must"] { background: var(--red); }
.level-dot[data-level="should"] { background: var(--yellow); }
.level-dot[data-level="may"] { background: var(--accent); }
```

## Implementation Steps

### Step 1: Define Data Attribute Styles

Create a new section in the CSS (or a separate file `_data-variants.css`) with all data attribute selectors.

### Step 2: Update Components Incrementally

For each component:
1. Add data attribute support
2. Keep old class-based styles temporarily
3. Update HTML/JSX to use data attributes
4. Remove old class-based styles

### Step 3: Update TypeScript/JSX

Example React component changes:

```tsx
// Before
<span className={`folder-badge ${status}`}>

// After  
<span className="badge" data-status={status}>
```

### Step 4: Clean Up

Remove deprecated class-based variant styles.

## Benefits

1. **Reduced duplication**: One set of variant styles works everywhere
2. **Explicit semantics**: `data-status="covered"` is clearer than `.full`
3. **No class collisions**: Data attributes are namespaced
4. **Easier JavaScript access**: `element.dataset.status` is clean
5. **CSS specificity**: Attribute selectors have same specificity as classes

## Considerations

- **Performance**: Attribute selectors are slightly slower than classes, but negligible for this scale
- **Browser support**: Excellent - works everywhere
- **Migration**: Can be done incrementally, component by component

## Estimated Impact

- **Lines removed**: ~80-100 lines of duplicate variant definitions
- **Clarity**: High improvement in understanding variant relationships
- **Maintainability**: Single source of truth for each variant type

## Dependencies

- None (can be done before or after other refactoring)

## Testing Checklist

- [ ] Status badges display correctly in file tree
- [ ] Rule markers show correct status colors
- [ ] Requirement containers have proper borders/backgrounds
- [ ] Reference type icons colored correctly
- [ ] Stat values show quality colors
- [ ] Button variants work correctly
- [ ] Level dots in dropdown display properly