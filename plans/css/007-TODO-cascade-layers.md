# 007: Cascade Layers

## Overview

CSS Cascade Layers (`@layer`) provide explicit control over specificity and style ordering. This eliminates specificity wars and makes the cascade predictable, especially important as the codebase grows.

## Current Problems

1. **Specificity conflicts** - Utility classes sometimes need `!important` to override component styles
2. **Order dependence** - Moving CSS rules around can break styling unexpectedly
3. **Hard to override** - Third-party styles (CodeMirror) can conflict with our styles
4. **No clear hierarchy** - Reset, base, component, and utility styles are intermixed

## Layer Architecture

```css
/* Define layer order at the top of style.css */
@layer reset, tokens, base, components, utilities, overrides;
```

### Layer Purposes

| Layer | Purpose | Specificity |
|-------|---------|-------------|
| `reset` | Box-sizing, margin/padding reset | Lowest |
| `tokens` | CSS custom properties (`:root`) | N/A (no rules) |
| `base` | Element defaults (body, a, code) | Low |
| `components` | All component styles | Medium |
| `utilities` | Single-purpose helpers | High |
| `overrides` | Third-party fixes, edge cases | Highest |

## Implementation

### Step 1: Define Layer Order

```css
/* style.css - very first lines */
@layer reset, tokens, base, components, utilities, overrides;

/* Later: wrap each section */
```

### Step 2: Reset Layer

```css
@layer reset {
    *,
    *::before,
    *::after {
        box-sizing: border-box;
        margin: 0;
        padding: 0;
    }
}
```

### Step 3: Tokens Layer

```css
@layer tokens {
    :root {
        color-scheme: light dark;
        
        /* All CSS custom properties */
        --bg: light-dark(#f8f9fa, #141822);
        --fg: light-dark(#1a1b26, #d0d4da);
        /* ... etc ... */
    }
}
```

### Step 4: Base Layer

```css
@layer base {
    body {
        font-family: var(--font-sans);
        background: var(--bg-outer);
        color: var(--fg);
        line-height: 1.6;
    }

    a {
        color: var(--accent);
        text-decoration: none;
    }

    code, pre {
        font-family: var(--font-mono);
        font-variation-settings: "MONO" 1, "CASL" 0;
    }
}
```

### Step 5: Components Layer (with Sub-Layers)

```css
@layer components {
    /* Sub-layers for component organization */
    @layer layout, navigation, sidebar, content, forms, modals;
}

@layer components.layout {
    .layout { /* ... */ }
    .header { /* ... */ }
    .main { /* ... */ }
}

@layer components.navigation {
    .nav { /* ... */ }
    .nav-tab { /* ... */ }
}

@layer components.sidebar {
    .sidebar { /* ... */ }
    .file-tree { /* ... */ }
    .toc-item { /* ... */ }
}

@layer components.content {
    .code-view { /* ... */ }
    .markdown { /* ... */ }
    .rule-marker { /* ... */ }
}

@layer components.forms {
    .btn { /* ... */ }
    .search-input { /* ... */ }
    .custom-dropdown { /* ... */ }
}

@layer components.modals {
    .modal-overlay { /* ... */ }
    .search-overlay { /* ... */ }
}
```

### Step 6: Utilities Layer

```css
@layer utilities {
    /* From 002-utility-classes.md */
    .flex { display: flex; }
    .hidden { display: none; }
    .mono { font-family: var(--font-mono); }
    /* ... */
}
```

### Step 7: Overrides Layer

```css
@layer overrides {
    /* CodeMirror customizations */
    .cm-editor {
        font-family: var(--font-mono);
        background: var(--bg);
    }
    
    /* Edge case fixes */
    .markdown pre code {
        background: none;
        padding: 0;
    }
    
    /* Print styles */
    @media print {
        .sidebar, .header { display: none; }
    }
}
```

## Benefits

### 1. Predictable Cascade

```css
/* Utilities always win over components - no !important needed */
@layer components {
    .btn { display: inline-flex; }
}

@layer utilities {
    .hidden { display: none; }  /* Always wins */
}
```

### 2. Safe Third-Party Integration

```css
/* Import external styles into a controlled layer */
@import url("codemirror.css") layer(external);

/* Our overrides always apply */
@layer overrides {
    .cm-editor { /* ... */ }
}
```

### 3. Easier Debugging

DevTools show which layer a rule comes from, making cascade issues obvious.

### 4. Component Isolation

```css
/* Components can't accidentally override each other */
@layer components.sidebar {
    .header { /* Sidebar's header */ }
}

@layer components.modals {
    .header { /* Modal's header - no conflict */ }
}
```

## Migration Strategy

### Phase 1: Add Layer Declarations (Non-Breaking)

1. Add `@layer` order declaration at top of file
2. Don't wrap anything yet - unlayered styles beat layered styles

### Phase 2: Wrap Reset and Base

```css
@layer reset { /* wrap existing reset */ }
@layer base { /* wrap body, a, code defaults */ }
```

### Phase 3: Wrap Components Section by Section

Work through component groups, testing each:
1. Layout components
2. Navigation
3. Sidebar
4. Content area
5. Forms/buttons
6. Modals

### Phase 4: Add Utilities Layer

Move/create utility classes in the utilities layer.

### Phase 5: Add Overrides

Move edge cases and third-party fixes to overrides layer.

## File Organization Options

### Option A: Single File with Layers

Keep everything in `style.css` with clear layer sections (simpler).

### Option B: Multiple Files with Layer Imports

```css
/* style.css */
@layer reset, tokens, base, components, utilities, overrides;

@import "reset.css" layer(reset);
@import "tokens.css" layer(tokens);
@import "base.css" layer(base);
@import "components/index.css" layer(components);
@import "utilities.css" layer(utilities);
@import "overrides.css" layer(overrides);
```

**Recommendation**: Start with Option A, consider Option B if the file becomes unwieldy.

## Browser Support

- Chrome 99+
- Firefox 97+
- Safari 15.4+
- Edge 99+

All modern browsers support cascade layers. No polyfill needed.

## Testing Checklist

- [ ] Layer order declaration is first in file
- [ ] Reset styles apply correctly
- [ ] Base element styles work
- [ ] Components render as before
- [ ] Utilities override components without `!important`
- [ ] CodeMirror integration still works
- [ ] No visual regressions
- [ ] DevTools show correct layer attribution

## Related Plans

- **002-utility-classes.md** - Utilities go in the utilities layer
- **003-data-attributes.md** - Component variants stay in components layer
- **009-file-organization.md** - Layer structure guides file splitting