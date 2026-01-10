# 011: Typography System

## Overview

Create a systematic typography scale using CSS custom properties and modern font features.

## Current State

Typography values scattered throughout:
- Font sizes: `0.6rem`, `0.65rem`, `0.7rem`, `0.75rem`, `0.8rem`, `0.85rem`, `0.875rem`, `0.9rem`, `0.95rem`, `1rem`, `1.1rem`, `1.25rem`, `1.5rem`
- Line heights: `1.4`, `1.5`, `1.6`, `1.75`
- Font weights: `400`, `500`, `600`, `700`, `750`, `800`, `850`, `900`

## Proposed Type Scale

```css
:root {
    /* Type scale using perfect fourth (1.333) ratio */
    --text-2xs: 0.625rem;   /* 10px - badges, tiny labels */
    --text-xs: 0.75rem;     /* 12px - captions, metadata */
    --text-sm: 0.875rem;    /* 14px - secondary text, UI */
    --text-base: 1rem;      /* 16px - body text */
    --text-lg: 1.125rem;    /* 18px - lead paragraphs */
    --text-xl: 1.25rem;     /* 20px - h4 */
    --text-2xl: 1.5rem;     /* 24px - h3 */
    --text-3xl: 1.875rem;   /* 30px - h2 */
    --text-4xl: 2.25rem;    /* 36px - h1 */
    
    /* Line heights */
    --leading-none: 1;
    --leading-tight: 1.25;
    --leading-snug: 1.375;
    --leading-normal: 1.5;
    --leading-relaxed: 1.625;
    --leading-loose: 2;
    
    /* Font weights - simplified */
    --weight-normal: 400;
    --weight-medium: 500;
    --weight-semibold: 600;
    --weight-bold: 700;
    --weight-extrabold: 800;
    
    /* Letter spacing */
    --tracking-tight: -0.025em;
    --tracking-normal: 0;
    --tracking-wide: 0.025em;
    --tracking-wider: 0.05em;
    --tracking-widest: 0.1em;
}
```

## Semantic Text Styles

```css
/* Composite text styles */
:root {
    /* Headings */
    --text-heading-1: var(--weight-extrabold) var(--text-2xl) / var(--leading-tight) var(--font-sans);
    --text-heading-2: var(--weight-bold) var(--text-xl) / var(--leading-tight) var(--font-sans);
    --text-heading-3: var(--weight-semibold) var(--text-lg) / var(--leading-snug) var(--font-sans);
    --text-heading-4: var(--weight-semibold) var(--text-base) / var(--leading-snug) var(--font-sans);
    
    /* Body */
    --text-body: var(--weight-normal) var(--text-base) / var(--leading-relaxed) var(--font-sans);
    --text-body-sm: var(--weight-normal) var(--text-sm) / var(--leading-normal) var(--font-sans);
    
    /* UI text */
    --text-ui: var(--weight-medium) var(--text-sm) / var(--leading-normal) var(--font-sans);
    --text-ui-sm: var(--weight-medium) var(--text-xs) / var(--leading-normal) var(--font-sans);
    
    /* Labels */
    --text-label: var(--weight-medium) var(--text-xs) / var(--leading-none) var(--font-sans);
    --text-label-caps: var(--weight-medium) var(--text-xs) / var(--leading-none) var(--font-sans);
    
    /* Code */
    --text-code: var(--weight-normal) var(--text-sm) / var(--leading-relaxed) var(--font-mono);
    --text-code-sm: var(--weight-normal) var(--text-xs) / var(--leading-relaxed) var(--font-mono);
}
```

## Migration Examples

### Before

```css
.stat-label {
    font-size: 0.75rem;
    color: var(--fg-muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
}

.markdown h1 {
    font-size: 1.5rem;
    font-weight: 900;
}

.tree-file {
    font-size: 0.875rem;
}

.rules-table th {
    font-size: 0.75rem;
    font-weight: 500;
    text-transform: uppercase;
    letter-spacing: 0.05em;
}
```

### After

```css
.stat-label {
    font: var(--text-label-caps);
    color: var(--fg-muted);
    text-transform: uppercase;
    letter-spacing: var(--tracking-wider);
}

.markdown h1 {
    font: var(--text-heading-1);
}

.tree-file {
    font-size: var(--text-sm);
}

.rules-table th {
    font: var(--text-label-caps);
    text-transform: uppercase;
    letter-spacing: var(--tracking-wider);
}
```

## Variable Font Features

Leverage Recursive font's variable axes:

```css
:root {
    /* Recursive font axes */
    --font-mono-settings: "MONO" 1, "CASL" 0;
    --font-casual-settings: "MONO" 0, "CASL" 1;
    --font-casual-mono-settings: "MONO" 1, "CASL" 0.5;
}

/* Utility classes for font variations */
.mono {
    font-family: var(--font-mono);
    font-variation-settings: var(--font-mono-settings);
}

.casual {
    font-variation-settings: var(--font-casual-settings);
}

/* Contextual adjustments */
.code-view {
    font-variation-settings: var(--font-mono-settings);
    font-feature-settings: "liga" 1, "calt" 1;
}
```

## Text Utility Classes

```css
/* Size utilities */
.text-2xs { font-size: var(--text-2xs); }
.text-xs { font-size: var(--text-xs); }
.text-sm { font-size: var(--text-sm); }
.text-base { font-size: var(--text-base); }
.text-lg { font-size: var(--text-lg); }
.text-xl { font-size: var(--text-xl); }

/* Weight utilities */
.font-normal { font-weight: var(--weight-normal); }
.font-medium { font-weight: var(--weight-medium); }
.font-semibold { font-weight: var(--weight-semibold); }
.font-bold { font-weight: var(--weight-bold); }

/* Leading utilities */
.leading-tight { line-height: var(--leading-tight); }
.leading-normal { line-height: var(--leading-normal); }
.leading-relaxed { line-height: var(--leading-relaxed); }

/* Tracking utilities */
.tracking-tight { letter-spacing: var(--tracking-tight); }
.tracking-wide { letter-spacing: var(--tracking-wide); }
.uppercase-label {
    text-transform: uppercase;
    letter-spacing: var(--tracking-wider);
}
```

## Responsive Typography

```css
/* Fluid type scale for headings */
.markdown h1 {
    font-size: clamp(var(--text-xl), 4vw, var(--text-2xl));
}

.markdown h2 {
    font-size: clamp(var(--text-lg), 3vw, var(--text-xl));
}

/* Adjust base size for small screens */
@media (max-width: 640px) {
    :root {
        --text-base: 0.9375rem; /* 15px on mobile */
    }
}
```

## Consolidation Targets

### Repeated Patterns to Unify

| Pattern | Occurrences | Replace With |
|---------|-------------|--------------|
| `font-size: 0.75rem` + uppercase + letter-spacing | 3 | `var(--text-label-caps)` |
| `font-size: 0.85rem` | 15+ | `var(--text-sm)` |
| `font-size: 0.875rem` | 8 | `var(--text-sm)` |
| `font-weight: 500` | 12 | `var(--weight-medium)` |
| `font-weight: 600` | 8 | `var(--weight-semibold)` |
| `line-height: 1.6` | 5 | `var(--leading-relaxed)` |

## Implementation Steps

1. **Add type scale variables** to `:root`
2. **Add semantic text style variables**
3. **Create text utility layer**
4. **Migrate headings** (`.markdown h1-h4`)
5. **Migrate labels** (`.stat-label`, `.rules-table th`, etc.)
6. **Migrate body text** sizes
7. **Migrate UI text** (buttons, tabs, badges)
8. **Migrate code/mono text**
9. **Add responsive typography** where beneficial
10. **Document** font variation settings usage

## Benefits

1. **Consistency**: All sizes from controlled scale
2. **Maintainability**: Change scale in one place
3. **Semantics**: `--text-label` is clearer than `0.75rem`
4. **Variable fonts**: Better use of Recursive's features
5. **Responsive**: Foundation for fluid typography
6. **Reduced decisions**: Fewer arbitrary values

## Notes

- Keep the scale tight (not too many steps)
- `0.8rem`, `0.85rem`, `0.9rem` can all become `--text-sm` (0.875rem)
- Round to scale steps rather than preserving exact values
- Font shorthand (`font:`) is powerful but be careful with inheritance