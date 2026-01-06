import { useCallback, useEffect, useMemo, useRef, useState } from "preact/hooks";
import { render } from "preact";
import { EDITORS } from "../config";
import { useSpec } from "../hooks";
import { CoverageArc, html, showRefsPopup } from "../main";
import type { OutlineEntry, SpecViewProps } from "../types";
import { MarkdownEditor } from "../components/MarkdownEditor";
import { InlineEditor } from "../components/InlineEditor";

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

    // Pop stack until we find a parent with lower level
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

// Aggregate coverage stats for a node and all its descendants
interface AggregatedCoverage {
  total: number;
  implCount: number;
  verifyCount: number;
}

function aggregateCoverage(node: OutlineTreeNode): AggregatedCoverage {
  // Start with this node's own stats
  let total = node.entry.aggregated.total;
  let implCount = node.entry.aggregated.implCount;
  let verifyCount = node.entry.aggregated.verifyCount;

  // Add children's aggregated stats
  for (const child of node.children) {
    const childStats = aggregateCoverage(child);
    total += childStats.total;
    implCount += childStats.implCount;
    verifyCount += childStats.verifyCount;
  }

  return { total, implCount, verifyCount };
}

// Recursive outline tree renderer
interface OutlineTreeProps {
  nodes: OutlineTreeNode[];
  activeHeading: string | null;
  specName: string | null;
  impl: string | null;
  onSelectHeading: (slug: string) => void;
  depth?: number;
}

function OutlineTree({
  nodes,
  activeHeading,
  specName,
  impl,
  onSelectHeading,
  depth = 0,
}: OutlineTreeProps) {
  return html`
    ${nodes.map((node) => {
      const isActive = node.entry.slug === activeHeading;
      const hasActiveChild = isActiveOrHasActiveChild(node, activeHeading);
      const hasChildren = node.children.length > 0;
      const h = node.entry;

      // Aggregate coverage from this node and all descendants
      const coverage = aggregateCoverage(node);
      const showCoverage = coverage.total > 0;

      return html`
        <li
          key=${h.slug}
          class="toc-item depth-${depth} ${isActive ? "is-active" : ""} ${hasActiveChild
            ? "is-in-active-branch"
            : ""}"
        >
          <div class="toc-row">
            <a href=${`/${specName}/${impl}/spec#${h.slug}`} class="toc-link"> ${h.title} </a>
            ${showCoverage &&
            html`
              <span class="toc-badges" aria-label="coverage">
                <${CoverageArc}
                  count=${coverage.implCount}
                  total=${coverage.total}
                  color="var(--green)"
                  title="Impl: ${coverage.implCount}/${coverage.total}"
                  hideNumber
                />
                <${CoverageArc}
                  count=${coverage.verifyCount}
                  total=${coverage.total}
                  color="var(--blue)"
                  title="Tests: ${coverage.verifyCount}/${coverage.total}"
                  hideNumber
                />
              </span>
            `}
          </div>
          ${hasChildren &&
          html`
            <ul class="toc-children ${hasActiveChild ? "has-active" : ""}">
              <${OutlineTree}
                nodes=${node.children}
                activeHeading=${activeHeading}
                specName=${specName}
                impl=${impl}
                onSelectHeading=${onSelectHeading}
                depth=${depth + 1}
              />
            </ul>
          `}
        </li>
      `;
    })}
  `;
}

// Declare lucide as global
declare const lucide: { createIcons: (opts?: { nodes?: NodeList }) => void };

// [impl dashboard.spec.outline]
// [impl dashboard.spec.outline-coverage]
// [impl dashboard.spec.content]
// [impl dashboard.spec.rule-highlight]
// [impl dashboard.spec.heading-scroll]
// [impl dashboard.links.heading-links]
// [impl dashboard.links.spec-aware]
export function SpecView({
  config,
  version,
  selectedSpec,
  selectedImpl,
  selectedRule,
  selectedHeading,
  onSelectSpec,
  onSelectRule,
  onSelectFile,
  scrollPosition,
  onScrollChange,
}: SpecViewProps) {
  // Use selectedSpec or default to first spec
  const specName = selectedSpec || config.specs?.[0]?.name || null;
  const spec = useSpec(specName, version);
  const [activeHeading, setActiveHeading] = useState<string | null>(null);
  const contentRef = useRef<HTMLDivElement>(null);
  const contentBodyRef = useRef<HTMLDivElement>(null);
  const initialScrollPosition = useRef(scrollPosition);
  const lastScrolledHeading = useRef<string | null>(null);

  // Markdown editor modal state
  const [editorState, setEditorState] = useState<{
    filePath: string;
    byteRange: string;
  } | null>(null);

  // Inline editor state
  const editingContainerRef = useRef<{
    element: HTMLElement;
    originalHTML: string;
    placeholder?: Comment;
  } | null>(null);

  // Use outline from API (already has coverage info)
  const outline = spec?.outline || [];

  // Build hierarchical tree from flat outline
  const outlineTree = useMemo(() => buildOutlineTree(outline), [outline]);

  // Concatenate all sections' HTML (sections are pre-sorted by weight on server)
  const processedContent = useMemo(() => {
    if (!spec?.sections) return "";
    return spec.sections.map((s) => s.html).join("\n");
  }, [spec?.sections]);

  // Set up scroll-based heading tracking
  useEffect(() => {
    if (!contentRef.current || !contentBodyRef.current || outline.length === 0) return;

    const contentBody = contentBodyRef.current;

    const updateActiveHeading = () => {
      const headingElements = contentRef.current?.querySelectorAll(
        "h1[id], h2[id], h3[id], h4[id]",
      );
      if (!headingElements || headingElements.length === 0) return;

      const scrollTop = contentBody.scrollTop;
      const viewportTop = 100;

      let activeId: string | null = null;

      for (const el of headingElements) {
        const htmlEl = el as HTMLElement;
        const offsetTop = htmlEl.offsetTop;

        if (offsetTop <= scrollTop + viewportTop) {
          activeId = htmlEl.id;
        } else {
          break;
        }
      }

      if (!activeId && headingElements.length > 0) {
        activeId = (headingElements[0] as HTMLElement).id;
      }

      if (activeId) {
        setActiveHeading(activeId);
      }
    };

    const timeoutId = setTimeout(updateActiveHeading, 100);

    contentBody.addEventListener("scroll", updateActiveHeading, {
      passive: true,
    });

    return () => {
      clearTimeout(timeoutId);
      contentBody.removeEventListener("scroll", updateActiveHeading);
    };
  }, [processedContent, outline]);

  // Track scroll position changes
  useEffect(() => {
    if (!contentBodyRef.current) return;

    const handleScroll = () => {
      if (onScrollChange && contentBodyRef.current) {
        onScrollChange(contentBodyRef.current.scrollTop);
      }
    };

    contentBodyRef.current.addEventListener("scroll", handleScroll, {
      passive: true,
    });
    return () => contentBodyRef.current?.removeEventListener("scroll", handleScroll);
  }, [onScrollChange]);

  // Initialize Lucide icons after content renders
  useEffect(() => {
    if (processedContent && contentRef.current && typeof lucide !== "undefined") {
      requestAnimationFrame(() => {
        lucide.createIcons({
          nodes: contentRef.current?.querySelectorAll("[data-lucide]"),
        });
      });
    }
  }, [processedContent]);

  // Add pencil edit buttons to paragraphs with data-source-file/data-source-line
  useEffect(() => {
    if (!processedContent || !contentRef.current || !config) return;

    const elements = contentRef.current.querySelectorAll("[data-source-file][data-source-line]");

    for (const el of elements) {
      if (el.querySelector(".para-edit-btn")) continue;

      const sourceFile = el.getAttribute("data-source-file");
      const sourceLine = el.getAttribute("data-source-line");
      if (!sourceFile || !sourceLine) continue;

      // Use sourceFile directly if absolute, otherwise prepend projectRoot
      const fullPath = sourceFile.startsWith("/")
        ? sourceFile
        : config.projectRoot
          ? `${config.projectRoot}/${sourceFile}`
          : sourceFile;
      const editUrl = EDITORS.zed.urlTemplate(fullPath, parseInt(sourceLine, 10));

      const btn = document.createElement("a");
      btn.className = "para-edit-btn";
      btn.href = editUrl;
      btn.title = `Edit in Zed (${sourceFile}:${sourceLine})`;
      btn.innerHTML = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M17 3a2.85 2.83 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5Z"/><path d="m15 5 4 4"/></svg>`;

      el.appendChild(btn);
    }
  }, [processedContent, config]);

  const scrollToHeading = useCallback((slug: string) => {
    if (!contentRef.current || !contentBodyRef.current) return;
    const el = contentRef.current.querySelector(`[id="${slug}"]`);
    if (el) {
      const targetScrollTop = (el as HTMLElement).offsetTop - 100;
      contentBodyRef.current.scrollTo({ top: Math.max(0, targetScrollTop) });
      setActiveHeading(slug);
    }
  }, []);

  // Handle clicks on headings, rule markers, anchor links, and spec refs
  useEffect(() => {
    if (!contentRef.current) return;

    const handleClick = (e: Event) => {
      const target = e.target as HTMLElement;

      // Handle heading clicks (copy URL and scroll)
      const heading = target.closest("h1[id], h2[id], h3[id], h4[id]");
      if (heading) {
        const slug = heading.id;
        const url = `${window.location.origin}${window.location.pathname}#${slug}`;
        navigator.clipboard?.writeText(url);
        history.pushState(null, "", `#${slug}`);
        setActiveHeading(slug);
        // Scroll the heading to a comfortable position (not at very top)
        const targetScrollTop = (heading as HTMLElement).offsetTop - 100;
        contentBodyRef.current?.scrollTo({ top: Math.max(0, targetScrollTop) });
        return;
      }

      // Handle rule marker clicks
      const ruleMarker = target.closest("a.rule-marker[data-rule]") as HTMLElement | null;
      if (ruleMarker) {
        e.preventDefault();
        const ruleId = ruleMarker.dataset.rule;
        if (ruleId) onSelectRule(ruleId);
        return;
      }

      // Handle Edit badge clicks - mount inline editor
      const editBadge = target.closest("button.req-badge.req-edit") as HTMLElement | null;
      if (editBadge) {
        e.preventDefault();
        const sourceFile = editBadge.dataset.sourceFile;
        const byteRange = editBadge.dataset.br;
        if (sourceFile && byteRange) {
          // Find the req-container
          const reqContainer = editBadge.closest(".req-container") as HTMLElement | null;
          if (reqContainer && reqContainer.parentElement) {
            // Save original element and its position
            const placeholder = document.createComment("editor-placeholder");
            reqContainer.parentElement.insertBefore(placeholder, reqContainer);
            const originalElement = reqContainer;

            editingContainerRef.current = {
              element: originalElement,
              originalHTML: "", // Not used - we'll restore the whole element
              placeholder,
            };

            // Remove the original element
            originalElement.remove();

            // Create container for editor
            const editorContainer = document.createElement("div");
            placeholder.parentElement?.insertBefore(editorContainer, placeholder);

            // Mount InlineEditor
            render(
              html`<${InlineEditor}
                filePath=${sourceFile}
                byteRange=${byteRange}
                onSave=${() => {
                  // Unmount editor and restore original element
                  if (editingContainerRef.current) {
                    render(null, editorContainer);
                    editorContainer.remove();
                    editingContainerRef.current.placeholder.parentElement?.insertBefore(
                      editingContainerRef.current.element,
                      editingContainerRef.current.placeholder,
                    );
                    editingContainerRef.current.placeholder.remove();
                    editingContainerRef.current = null;
                  }
                }}
                onCancel=${() => {
                  // Unmount editor and restore original element
                  if (editingContainerRef.current) {
                    render(null, editorContainer);
                    editorContainer.remove();
                    editingContainerRef.current.placeholder.parentElement?.insertBefore(
                      editingContainerRef.current.element,
                      editingContainerRef.current.placeholder,
                    );
                    editingContainerRef.current.placeholder.remove();
                    editingContainerRef.current = null;
                  }
                }}
              />`,
              editorContainer,
            );
          }
        }
        return;
      }

      // Handle rule-id badge clicks - open spec source in editor
      const ruleBadge = target.closest(
        "a.rule-badge.rule-id[data-source-file][data-source-line]",
      ) as HTMLElement | null;
      if (ruleBadge) {
        e.preventDefault();
        const sourceFile = ruleBadge.dataset.sourceFile;
        const sourceLine = parseInt(ruleBadge.dataset.sourceLine || "0", 10);
        if (sourceFile && !Number.isNaN(sourceLine)) {
          // Use sourceFile directly if absolute, otherwise prepend projectRoot
          const fullPath = sourceFile.startsWith("/")
            ? sourceFile
            : config.projectRoot
              ? `${config.projectRoot}/${sourceFile}`
              : sourceFile;
          window.location.href = EDITORS.zed.urlTemplate(fullPath, sourceLine);
        }
        return;
      }

      // Handle impl/test badge clicks with multiple refs - show popup
      const refBadge = target.closest("a.rule-badge[data-all-refs]") as HTMLElement | null;
      if (refBadge) {
        const allRefsJson = refBadge.dataset.allRefs;
        if (allRefsJson) {
          try {
            const refs = JSON.parse(allRefsJson);
            if (refs.length > 1) {
              e.preventDefault();
              showRefsPopup(e, refs, refBadge, onSelectFile);
              return;
            }
          } catch (err) {
            console.error("Failed to parse refs:", err);
          }
        }
      }

      // Handle spec ref clicks - pass rule context
      const specRef = target.closest("a.spec-ref") as HTMLElement | null;
      if (specRef) {
        e.preventDefault();
        const file = specRef.dataset.file;
        const line = parseInt(specRef.dataset.line || "0", 10);
        const ruleBlock = specRef.closest(".rule-block");
        const ruleMarkerEl = ruleBlock?.querySelector(
          "a.rule-marker[data-rule]",
        ) as HTMLElement | null;
        const ruleContext = ruleMarkerEl?.dataset.rule || null;
        if (file) onSelectFile(file, line, ruleContext);
        return;
      }

      // Handle other anchor links (internal navigation)
      const anchor = target.closest("a[href]") as HTMLAnchorElement | null;
      if (anchor) {
        const href = anchor.getAttribute("href");
        if (!href) return;

        try {
          const url = new URL(href, window.location.href);
          if (url.origin === window.location.origin) {
            e.preventDefault();
            history.pushState(null, "", url.pathname + url.search + url.hash);
            window.dispatchEvent(new PopStateEvent("popstate"));
            return;
          }
        } catch {
          // Invalid URL, ignore
        }
      }
    };

    contentRef.current.addEventListener("click", handleClick);
    return () => contentRef.current?.removeEventListener("click", handleClick);
  }, [processedContent, onSelectRule, onSelectFile, config]);

  // Scroll to selected rule or heading, or restore scroll position
  useEffect(() => {
    if (!processedContent) return;

    let cancelled = false;
    requestAnimationFrame(() => {
      if (cancelled) return;
      requestAnimationFrame(() => {
        if (cancelled || !contentRef.current || !contentBodyRef.current) return;

        if (selectedRule) {
          const ruleEl = contentRef.current.querySelector(`[data-rule="${selectedRule}"]`);
          if (ruleEl) {
            const containerRect = contentBodyRef.current.getBoundingClientRect();
            const ruleRect = ruleEl.getBoundingClientRect();
            const currentScroll = contentBodyRef.current.scrollTop;
            const targetScrollTop = currentScroll + (ruleRect.top - containerRect.top) - 150;
            contentBodyRef.current.scrollTo({
              top: Math.max(0, targetScrollTop),
            });

            ruleEl.classList.add("rule-marker-highlighted");
            setTimeout(() => {
              ruleEl.classList.remove("rule-marker-highlighted");
            }, 3000);
          }
        } else if (selectedHeading && selectedHeading !== lastScrolledHeading.current) {
          lastScrolledHeading.current = selectedHeading;
          const headingEl = contentRef.current.querySelector(`[id="${selectedHeading}"]`);
          if (headingEl) {
            const targetScrollTop = (headingEl as HTMLElement).offsetTop - 100;
            contentBodyRef.current.scrollTo({
              top: Math.max(0, targetScrollTop),
            });
            setActiveHeading(selectedHeading);
          }
        } else if (initialScrollPosition.current > 0) {
          contentBodyRef.current.scrollTo({
            top: initialScrollPosition.current,
          });
          initialScrollPosition.current = 0;
        }
      });
    });

    return () => {
      cancelled = true;
    };
  }, [selectedRule, selectedHeading, processedContent]);

  if (!spec) {
    return html`
      <div class="main">
        <div class="empty-state">Loading spec...</div>
      </div>
    `;
  }

  return html`
    <div class="main">
      <div class="sidebar">
        <div class="sidebar-header">
          <span>Outline</span>
          <span class="outline-legend">
            <span class="legend-item"><span class="legend-dot legend-dot--impl"></span>Impl</span>
            <span class="legend-item"><span class="legend-dot legend-dot--test"></span>Test</span>
          </span>
        </div>
        <div class="sidebar-content">
          <ul class="outline-tree">
            <${OutlineTree}
              nodes=${outlineTree}
              activeHeading=${activeHeading}
              specName=${specName}
              impl=${selectedImpl}
              onSelectHeading=${scrollToHeading}
            />
          </ul>
        </div>
      </div>
      <div class="content">
        <div class="content-body" ref=${contentBodyRef}>
          <div
            class="markdown"
            ref=${contentRef}
            dangerouslySetInnerHTML=${{ __html: processedContent }}
          />
        </div>
      </div>
      ${
        editorState &&
        html`<${MarkdownEditor}
          filePath=${editorState.filePath}
          byteRange=${editorState.byteRange}
          onClose=${() => setEditorState(null)}
        />`
      }
    </div>
  `;
}
