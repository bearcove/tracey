# CSS Modernization Plan

## Overview

This plan outlines a comprehensive modernization of `style.css` (~2660 lines) to leverage modern CSS features, reduce repetition, and improve maintainability.

## Task Tracker

| # | Status | Topic | Impact | Complexity | File |
|---|--------|-------|--------|------------|------|
| 001 | DONE | CSS Nesting | High | Low | `001-DONE-nesting.md` |
| 002 | TODO | Utility Classes | Medium | Low | `002-TODO-utility-classes.md` |
| 003 | TODO | Data Attribute Variants | High | Medium | `003-TODO-data-attributes.md` |
| 004 | DONE | Selector Grouping | Medium | Low | `004-DONE-selector-grouping.md` |
| 005 | DONE | Color System | Medium | Low | `005-DONE-color-system.md` |
| 006 | TODO | Logical Properties | Low | Low | `006-TODO-logical-properties.md` |
| 007 | TODO | Cascade Layers | Medium | Medium | `007-TODO-cascade-layers.md` |
| 008 | TODO | Container Queries | Medium | Medium | `008-TODO-container-queries.md` |
| 009 | TODO | @scope | Low | High | `009-TODO-scope.md` |
| 010 | TODO | Animations | Low | Low | `010-TODO-animations.md` |
| 011 | TODO | Typography | Medium | Low | `011-TODO-typography.md` |
| 012 | TODO | Spacing | Medium | Low | `012-TODO-spacing.md` |

## Progress

- **Completed**: 3/12
- **In Progress**: 0/12
- **Remaining**: 9/12

## Current State

The stylesheet is well-organized but contains:
- Significant selector repetition (especially for hover/active states)
- Many similar badge/status variant classes
- Repeated flex patterns throughout
- Hardcoded color-mix values that could be variables
- Traditional CSS structure (no nesting, layers, or scoping)

## Goals

1. **Reduce file size** by ~30-40% through consolidation
2. **Improve maintainability** with better organization
3. **Future-proof** with modern CSS features
4. **Preserve functionality** - all existing styles must continue to work
5. **Maintain readability** - don't over-abstract

## Browser Support Target

Modern evergreen browsers (Chrome 120+, Firefox 120+, Safari 17+):
- Native CSS nesting ✓
- `:is()` and `:where()` ✓
- `@layer` ✓
- Container queries ✓
- `color-mix()` ✓ (already in use)
- `light-dark()` ✓ (already in use)
- Logical properties ✓
- `@scope` (Safari 17.4+, may need fallbacks)

## Execution Order

### Phase 1: Low-risk, high-impact
- **001** CSS Nesting - biggest single win, no HTML changes
- **004** Selector Grouping - quick wins with `:is()` and `:where()`
- **005** Color System - consolidate color-mix patterns

### Phase 2: Foundation
- **011** Typography - establish type scale
- **012** Spacing - establish spacing scale
- **002** Utility Classes - extract common patterns

### Phase 3: Structural
- **007** Cascade Layers - organize the cascade
- **006** Logical Properties - RTL-ready

### Phase 4: Advanced
- **003** Data Attributes - requires HTML changes
- **008** Container Queries - responsive components
- **010** Animations - consolidate timing
- **009** @scope - component isolation (if browser support sufficient)

## Testing Strategy

After each task:
1. Visual regression testing on all views (spec, source, rules)
2. Light/dark mode verification
3. Responsive behavior check
4. Interactive states (hover, focus, active)

## Files Affected

- `crates/tracey/src/bridge/http/dashboard/src/style.css` (primary)
- Component TSX files (for data-attribute changes in 003)

## Success Metrics

- [ ] File size reduced by ≥25%
- [ ] No visual regressions
- [ ] Easier to add new badge/status variants
- [ ] Clear organization via layers or file structure
- [ ] Reduced selector specificity conflicts

## Notes

- Rename files from `XXX-TODO-*.md` to `XXX-DONE-*.md` when complete
- Update the task tracker table status accordingly
- Each task should be completable in one session