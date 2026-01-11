# Plan 008: Container Queries

## Overview

Container queries allow components to respond to their container's size rather than the viewport. This is perfect for reusable components that appear in different contexts (sidebar, main content, modals).

## Current Limitations

The sidebar, modals, and split panes all have fixed breakpoint assumptions:

```css
/* Current: responsive design based on viewport */
@media (max-width: 768px) {
    .sidebar { width: 100%; }
}
```

This doesn't work when:
- The sidebar width is user-adjustable
- Components appear in modals vs. main content
- Split panes have variable widths

## Implementation

### Step 1: Define Containers

```css
/* Mark containers that components should respond to */
.sidebar {
    container-type: inline-size;
    container-name: sidebar;
}

.split-pane {
    container-type: inline-size;
    container-name: pane;
}

.modal-body {
    container-type: inline-size;
    container-name: modal;
}

.content-body {
    container-type: inline-size;
    container-name: content;
}
```

### Step 2: Responsive Sidebar Components

```css
/* TOC adapts to sidebar width */
@container sidebar (max-width: 280px) {
    .toc-row {
        padding: 0.2rem 0.35rem;
    }
    
    .toc-link {
        font-size: 0.8rem;
    }
    
    .toc-badges {
        /* Stack vertically or hide */
        flex-direction: column;
        gap: 0.15rem;
    }
    
    .toc-badge {
        font-size: 0.6rem;
        padding: 0.05rem 0.25rem;
    }
}

@container sidebar (max-width: 200px) {
    /* Ultra-narrow: hide badges entirely */
    .toc-badges {
        display: none;
    }
    
    .toc-link {
        font-size: 0.75rem;
    }
}

/* File tree in narrow sidebar */
@container sidebar (max-width: 280px) {
    .tree-file-badge,
    .folder-badge {
        /* Show as dots instead of text */
        width: 8px;
        height: 8px;
        padding: 0;
        border-radius: 50%;
        font-size: 0;
    }
}
```

### Step 3: Responsive Split Pane Content

```css
/* Code view adapts to pane width */
@container pane (max-width: 500px) {
    .code-view {
        font-size: 0.7rem;
    }
    
    .line-number {
        width: 3rem;
        padding: 0 0.5rem;
    }
    
    .line-annotations {
        /* Collapse to just count */
        padding: 0 0.25rem;
    }
    
    .annotation-badges {
        max-width: 200px;
    }
}

@container pane (max-width: 400px) {
    .line-annotations {
        display: none;
    }
}

/* Markdown in narrow panes */
@container pane (max-width: 500px) {
    .markdown {
        padding: 0.5rem 1rem;
        font-size: 0.9rem;
    }
    
    .markdown h1 { font-size: 1.25rem; }
    .markdown h2 { font-size: 1.1rem; }
    .markdown h3 { font-size: 1rem; }
    
    .markdown pre {
        font-size: 0.75rem;
        padding: 0.5rem;
    }
}

/* Tables scroll horizontally in narrow containers */
@container pane (max-width: 600px) {
    .markdown table {
        display: block;
        overflow-x: auto;
    }
}
```

### Step 4: Requirement Containers

```css
/* Req containers adapt to available width */
.req-container {
    container-type: inline-size;
    container-name: requirement;
}

@container requirement (max-width: 400px) {
    .req-badges-left,
    .req-badges-right {
        position: static;
        margin-bottom: 0.5rem;
    }
    
    .req-badge {
        font-size: 0.65rem;
    }
}

@container requirement (max-width: 300px) {
    .req-badge.req-edit {
        /* Icon only */
        padding: 0.15rem;
    }
    
    .req-badge.req-edit span {
        display: none;
    }
}
```

### Step 5: Modal Content Responsiveness

```css
/* Editor modal adapts to modal size */
@container modal (max-width: 600px) {
    .modal-info {
        flex-direction: column;
        align-items: flex-start;
        gap: 0.25rem;
    }
    
    .modal-actions {
        width: 100%;
        justify-content: stretch;
    }
    
    .modal-btn {
        flex: 1;
    }
}

/* Search modal results */
@container modal (max-width: 400px) {
    .search-modal-result-code {
        display: none;
    }
    
    .search-modal-result-content {
        white-space: normal;
        line-height: 1.4;
    }
}
```

### Step 6: Rules Table Responsiveness

```css
.content-body {
    container-type: inline-size;
}

@container content (max-width: 700px) {
    .rules-table {
        /* Stack columns */
        display: block;
    }
    
    .rules-table thead {
        display: none;
    }
    
    .rules-table tr {
        display: block;
        padding: 0.75rem;
        border-bottom: 1px solid var(--border);
    }
    
    .rules-table td {
        display: block;
        padding: 0.25rem 0;
        border: none;
    }
    
    .rules-table td::before {
        content: attr(data-label);
        font-weight: 500;
        color: var(--fg-muted);
        display: block;
        font-size: 0.75rem;
        margin-bottom: 0.25rem;
    }
}
```

### Step 7: Header Adaptations

```css
.header {
    container-type: inline-size;
    container-name: header;
}

@container header (max-width: 800px) {
    .header-inner {
        flex-wrap: wrap;
    }
    
    .nav {
        order: 3;
        width: 100%;
        border-top: 1px solid var(--border);
    }
    
    .nav-tab {
        flex: 1;
        justify-content: center;
    }
}

@container header (max-width: 500px) {
    .nav-tab span {
        display: none;
    }
    
    .nav-tab .tab-icon {
        margin: 0;
    }
}
```

## Container Query Units

Use container-relative units for fluid sizing:

```css
/* cqi = 1% of container inline size */
@container sidebar (min-width: 200px) {
    .toc-link {
        /* Fluid font size based on sidebar width */
        font-size: clamp(0.75rem, 3cqi, 0.875rem);
    }
}

/* cqw, cqh for width/height */
.search-modal {
    container-type: size;
}

@container (min-height: 500px) {
    .search-modal-results {
        max-height: 60cqh;
    }
}
```

## Browser Support

Container queries are supported in all modern browsers (Chrome 105+, Firefox 110+, Safari 16+). No fallback needed for this project's target audience.

## Files to Modify

- `style.css` - Add container definitions and queries

## Benefits

1. **True component responsiveness** - Components adapt to actual available space
2. **Reusable anywhere** - Same component works in sidebar, modal, or main content
3. **User-adjustable layouts** - If you add resizable panes later, components just work
4. **Cleaner than media queries** - No guessing about viewport vs. actual container size

## Testing Checklist

- [ ] Resize browser with sidebar visible - TOC adapts
- [ ] Test split view with narrow panes - code view adapts
- [ ] Test search modal on narrow viewport - results stack properly
- [ ] Test requirement blocks in narrow markdown view
- [ ] Verify no layout shifts during resize