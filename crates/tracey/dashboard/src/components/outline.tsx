// Outline tree component for spec view sidebar

// Tree node for hierarchical outline
interface OutlineTreeNode {
  entry: OutlineEntry;
  children: OutlineTreeNode[];
}

// Convert flat outline to tree structure
function buildOutlineTree(outline: OutlineEntry[]): OutlineTreeNode[] {
  const roots: OutlineTreeNode[] = [];
  const stack: OutlineTreeNode[] = [];

  for (const entry of outline) {
    const node: OutlineTreeNode = { entry, children: [] };

    // Pop items from stack until we find a parent (lower level number)
    while (stack.length > 0 && stack[stack.length - 1].entry.level >= entry.level) {
      stack.pop();
    }

    if (stack.length === 0) {
      roots.push(node);
    } else {
      stack[stack.length - 1].children.push(node);
    }

    stack.push(node);
  }

  return roots;
}

// Check if a heading or any of its descendants is active
function isActiveOrHasActiveChild(node: OutlineTreeNode, activeHeading: string | null): boolean {
  if (node.entry.slug === activeHeading) return true;
  return node.children.some((child) => isActiveOrHasActiveChild(child, activeHeading));
}

// localStorage key for outline collapse state
const OUTLINE_COLLAPSE_KEY = "tracey-outline-collapsed";

// Get collapsed slugs from localStorage
function getCollapsedSlugs(): Set<string> {
  try {
    const stored = localStorage.getItem(OUTLINE_COLLAPSE_KEY);
    return stored ? new Set(JSON.parse(stored)) : new Set();
  } catch {
    return new Set();
  }
}

// Save collapsed slugs to localStorage
function saveCollapsedSlugs(slugs: Set<string>): void {
  try {
    localStorage.setItem(OUTLINE_COLLAPSE_KEY, JSON.stringify([...slugs]));
  } catch {
    // Ignore storage errors
  }
}

// Recursive outline tree renderer
interface OutlineTreeProps {
  nodes: OutlineTreeNode[];
  activeHeading: string | null;
  onSelectHeading: (slug: string) => void;
  collapsedSlugs: Set<string>;
  onToggleCollapse: (slug: string) => void;
  depth?: number;
}

function OutlineTree({
  nodes,
  activeHeading,
  onSelectHeading,
  collapsedSlugs,
  onToggleCollapse,
  depth = 0,
}: OutlineTreeProps) {
  return html`
    ${nodes.map((node) => {
      const isActive = node.entry.slug === activeHeading;
      const hasActiveChild = isActiveOrHasActiveChild(node, activeHeading);
      const hasChildren = node.children.length > 0;
      const isCollapsed = collapsedSlugs.has(node.entry.slug);
      const h = node.entry;

      // Only show coverage indicators if:
      // 1. There are rules AND
      // 2. Either no children OR is collapsed
      const showCoverage = h.aggregated.total > 0 && (!hasChildren || isCollapsed);

      return html`
        <div key=${h.slug} class="outline-node ${depth > 0 ? "outline-node-nested" : ""}">
          <div class="outline-item-row">
            ${hasChildren &&
            html`
              <button
                class="outline-toggle ${isCollapsed ? "collapsed" : ""}"
                onClick=${(e) => {
                  e.preventDefault();
                  e.stopPropagation();
                  onToggleCollapse(h.slug);
                }}
                title=${isCollapsed ? "Expand" : "Collapse"}
              >
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <path d="M9 18l6-6-6-6" />
                </svg>
              </button>
            `}
            <a
              href=${`/spec/${h.slug}`}
              class="outline-item ${isActive ? "active" : ""}"
              onClick=${(e) => {
                e.preventDefault();
                history.pushState(null, "", `/spec/${h.slug}`);
                onSelectHeading(h.slug);
              }}
            >
              <span class="outline-title">${h.title}</span>
              ${showCoverage &&
              html`
                <span class="outline-indicators">
                  <${CoverageArc}
                    count=${h.aggregated.implCount}
                    total=${h.aggregated.total}
                    color="var(--green)"
                    title="Implementation: ${h.aggregated.implCount}/${h.aggregated.total}"
                  />
                  <${CoverageArc}
                    count=${h.aggregated.verifyCount}
                    total=${h.aggregated.total}
                    color="var(--blue)"
                    title="Tests: ${h.aggregated.verifyCount}/${h.aggregated.total}"
                  />
                </span>
              `}
            </a>
          </div>
          ${hasChildren &&
          !isCollapsed &&
          html`
            <div class="outline-children ${hasActiveChild ? "has-active" : ""}">
              <${OutlineTree}
                nodes=${node.children}
                activeHeading=${activeHeading}
                onSelectHeading=${onSelectHeading}
                collapsedSlugs=${collapsedSlugs}
                onToggleCollapse=${onToggleCollapse}
                depth=${depth + 1}
              />
            </div>
          `}
        </div>
      `;
    })}
  `;
}
