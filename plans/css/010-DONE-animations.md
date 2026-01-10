# 010: Animation Consolidation

## Overview

The CSS contains several animation definitions scattered throughout. Consolidating these into a unified animation system with consistent timing and reusable keyframes will improve maintainability and create a more cohesive feel.

## Current State

### Existing Animations

```css
/* Rule highlight pulse */
@keyframes rule-highlight-pulse {
    0%, 20% {
        box-shadow: 0 0 0 4px var(--accent);
        transform: scale(1.05);
    }
    100% {
        box-shadow: 0 0 0 0 transparent;
        transform: scale(1);
    }
}

/* Copy success flash */
@keyframes req-copy-flash {
    0% {
        background: color-mix(in srgb, var(--green) 30%, var(--bg));
    }
    100% {
        background: transparent;
    }
}

/* Vim pending key pop */
@keyframes vim-pending-pop {
    0% {
        transform: scale(0.8);
        opacity: 0;
    }
    100% {
        transform: scale(1);
        opacity: 1;
    }
}
```

### Scattered Transitions

Transitions are defined inconsistently:
- `transition: background 0.15s`
- `transition: all 0.15s`
- `transition: opacity 0.15s, color 0.15s`
- `transition: transform 0.15s`
- No easing function specified (defaults to `ease`)

## Proposed Changes

### 1. Define Timing Variables

```css
:root {
    /* Duration scale */
    --duration-instant: 0.1s;
    --duration-fast: 0.15s;
    --duration-normal: 0.25s;
    --duration-slow: 0.4s;
    --duration-emphasis: 3s;
    
    /* Easing functions */
    --ease-out: cubic-bezier(0.16, 1, 0.3, 1);
    --ease-in-out: cubic-bezier(0.65, 0, 0.35, 1);
    --ease-bounce: cubic-bezier(0.34, 1.56, 0.64, 1);
    --ease-spring: cubic-bezier(0.175, 0.885, 0.32, 1.275);
}
```

### 2. Create Reusable Keyframes

```css
/* Attention-grabbing pulse for highlights */
@keyframes pulse-attention {
    0%, 20% {
        box-shadow: 0 0 0 4px var(--pulse-color, var(--accent));
        transform: scale(var(--pulse-scale, 1.05));
    }
    100% {
        box-shadow: 0 0 0 0 transparent;
        transform: scale(1);
    }
}

/* Flash for success feedback */
@keyframes flash-success {
    0% {
        background: color-mix(in srgb, var(--flash-color, var(--green)) 30%, var(--bg));
    }
    100% {
        background: transparent;
    }
}

/* Pop-in for appearing elements */
@keyframes pop-in {
    from {
        transform: scale(0.8);
        opacity: 0;
    }
    to {
        transform: scale(1);
        opacity: 1;
    }
}

/* Fade in */
@keyframes fade-in {
    from { opacity: 0; }
    to { opacity: 1; }
}

/* Slide in from direction */
@keyframes slide-in-up {
    from {
        transform: translateY(0.5rem);
        opacity: 0;
    }
    to {
        transform: translateY(0);
        opacity: 1;
    }
}

@keyframes slide-in-down {
    from {
        transform: translateY(-0.5rem);
        opacity: 0;
    }
    to {
        transform: translateY(0);
        opacity: 1;
    }
}
```

### 3. Animation Utility Classes

```css
/* Apply to elements that need animations */
.animate-pulse {
    animation: pulse-attention var(--duration-emphasis) var(--ease-out);
}

.animate-flash {
    animation: flash-success var(--duration-normal) var(--ease-out);
}

.animate-pop {
    animation: pop-in var(--duration-instant) var(--ease-spring);
}

.animate-fade-in {
    animation: fade-in var(--duration-fast) var(--ease-out);
}

.animate-slide-up {
    animation: slide-in-up var(--duration-fast) var(--ease-out);
}

.animate-slide-down {
    animation: slide-in-down var(--duration-fast) var(--ease-out);
}
```

### 4. Transition Utilities

```css
/* Common transition patterns */
.transition-colors {
    transition: 
        color var(--duration-fast) var(--ease-out),
        background-color var(--duration-fast) var(--ease-out),
        border-color var(--duration-fast) var(--ease-out);
}

.transition-opacity {
    transition: opacity var(--duration-fast) var(--ease-out);
}

.transition-transform {
    transition: transform var(--duration-fast) var(--ease-out);
}

.transition-all {
    transition: all var(--duration-fast) var(--ease-out);
}

/* Combined for interactive elements */
.transition-interactive {
    transition: 
        color var(--duration-fast) var(--ease-out),
        background-color var(--duration-fast) var(--ease-out),
        opacity var(--duration-fast) var(--ease-out),
        transform var(--duration-fast) var(--ease-out);
}
```

### 5. Respect User Preferences

```css
@media (prefers-reduced-motion: reduce) {
    *,
    *::before,
    *::after {
        animation-duration: 0.01ms !important;
        animation-iteration-count: 1 !important;
        transition-duration: 0.01ms !important;
    }
}
```

## Migration Examples

### Before

```css
.rule-marker-highlighted {
    animation: rule-highlight-pulse 3s ease-out;
}

.vim-pending-key {
    animation: vim-pending-pop 0.1s ease-out;
}

.req-container.req-copy-success {
    animation: req-copy-flash 0.3s ease-out;
}

.btn {
    transition: background 0.15s, opacity 0.15s;
}
```

### After

```css
.rule-marker-highlighted {
    animation: pulse-attention var(--duration-emphasis) var(--ease-out);
}

.vim-pending-key {
    animation: pop-in var(--duration-instant) var(--ease-spring);
}

.req-container.req-copy-success {
    animation: flash-success var(--duration-normal) var(--ease-out);
}

.btn {
    transition: 
        background-color var(--duration-fast) var(--ease-out),
        opacity var(--duration-fast) var(--ease-out);
}
```

## Patterns to Consolidate

| Current | Proposed Animation/Transition |
|---------|-------------------------------|
| `.rule-marker-highlighted` animation | `animation: pulse-attention ...` |
| `.req-container.req-copy-success` | `animation: flash-success ...` |
| `.vim-pending-key` animation | `animation: pop-in ...` |
| Hover transitions (multiple) | `@mixin transition-colors` or utility |
| Transform transitions | `@mixin transition-transform` |

## Benefits

1. **Consistency**: All animations use the same timing scale and easing functions
2. **Maintainability**: Change duration/easing in one place
3. **Accessibility**: Single location for reduced-motion handling
4. **Reusability**: Generic animations can be applied to new elements
5. **Performance**: Explicit property transitions instead of `all`

## Implementation Notes

- Avoid animating properties that trigger layout (width, height, top, left)
- Prefer transform and opacity for smooth 60fps animations
- Use `will-change` sparingly for known animations
- Consider using CSS custom properties for dynamic animation values

## Dependencies

- Requires: 003-custom-properties.md (for consistent variable naming)
- Enhances: 001-nesting.md (can nest animation applications)