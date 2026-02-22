import { useCallback, useEffect, useMemo, useRef, useState } from "preact/hooks";
import { render } from "preact";
import { EDITORS } from "../config";
import { useSpec } from "../hooks";
import { CoverageArc, html, showRefsPopup } from "../main";
import type { OutlineEntry, SpecViewProps, FileContent } from "../types";
import { MarkdownEditor } from "../components/MarkdownEditor";
import { InlineEditor } from "../components/InlineEditor";
import { CodeView } from "./sources";

// Tree node for hierarchical outline
interface OutlineTreeNode {
  entry: OutlineEntry;
  children: OutlineTreeNode[];
}

// r[impl dashboard.impl-preview.modal]
// r[impl dashboard.impl-preview.scroll-highlight]
// r[impl dashboard.impl-preview.open-in-sources]
interface ImplementationPreviewModalProps {
  fileData: FileContent;
  line: number;
  lineEnd: number;
  type: "impl" | "verify";
  config: { projectRoot?: string };
  onClose: () => void;
  onOpenInSources: () => void;
}

function ImplementationPreviewModal({
  fileData,
  line,
  lineEnd,
  type,
  config,
  onClose,
  onOpenInSources,
}: ImplementationPreviewModalProps) {
  // Prevent background scrolling
  useEffect(() => {
    document.body.style.overflow = "hidden";
    return () => {
      document.body.style.overflow = "";
    };
  }, []);

  // Close on Escape key
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        onClose();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [onClose]);

  console.log("Preview modal config.projectRoot:", config.projectRoot);

  return html`
    <div class="modal-overlay" onClick=${onClose}>
      <div class="modal-content impl-preview-modal" onClick=${(e: Event) => e.stopPropagation()}>
        <div class="modal-header">
          <h3>${fileData.path}:${line}</h3>
          <button class="modal-close" onClick=${onClose} title="Close (Esc)">×</button>
        </div>
        <div class="modal-body">
          <${CodeView}
            file=${fileData}
            config=${config}
            selectedLine=${line}
            selectedLineEnd=${lineEnd}
            selectedType=${type}
            onSelectRule=${() => {}}
          />
        </div>
        <div class="modal-footer">
          <button class="modal-btn modal-btn-cancel" onClick=${onClose}>Close</button>
          <button class="modal-btn modal-btn-primary" onClick=${onOpenInSources}>
            Open in Sources →
          </button>
        </div>
      </div>
    </div>
  `;
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
      const isComplete = coverage.total > 0 && coverage.implCount === coverage.total;
      const isIncomplete = coverage.total > 0 && coverage.implCount < coverage.total;

      return html`
        <li
          key=${h.slug}
          class="toc-item depth-${depth} ${isActive ? "is-active" : ""} ${hasActiveChild
            ? "is-in-active-branch"
            : ""} ${isComplete ? "is-complete" : ""} ${isIncomplete ? "is-incomplete" : ""}"
        >
          <a class="toc-row" href=${`/${specName}/${impl}/spec#${h.slug}`}>
            <span class="toc-link"> ${h.title} </span>
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
          </a>
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

// r[impl dashboard.spec.outline]
// r[impl dashboard.spec.outline-coverage]
// r[impl dashboard.spec.content]
// r[impl dashboard.spec.req-highlight]
// r[impl dashboard.spec.heading-scroll]
// r[impl dashboard.links.heading-links]
// r[impl dashboard.links.spec-aware]
// r[impl dashboard.editing.keyboard.navigation]
// r[impl dashboard.editing.keyboard.next-req]
// r[impl dashboard.editing.keyboard.prev-req]
// r[impl dashboard.editing.keyboard.visual-focus]
// r[impl dashboard.editing.keyboard.scope]
// r[impl dashboard.editing.keyboard.open-editor]
export function SpecView({
  config,
  version,
  selectedSpec,
  selectedImpl,
  selectedRule,
  selectedHeading,
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

  // Vim-style pending key indicator (for gg, yy, yl sequences)
  const [pendingKey, setPendingKey] = useState<string | null>(null);

  // Inline editor state
  const editingContainerRef = useRef<{
    element: HTMLElement;
    originalHTML: string;
    placeholder?: Comment;
  } | null>(null);

  // r[impl dashboard.impl-preview.modal]
  // Implementation preview modal state
  const [previewModal, setPreviewModal] = useState<{
    fileData: FileContent;
    line: number;
    lineEnd: number;
    type: "impl" | "verify";
  } | null>(null);

  // Use outline from API (already has coverage info)
  const outline = spec?.outline || [];

  // Build hierarchical tree from flat outline
  const outlineTree = useMemo(() => buildOutlineTree(outline), [outline]);

  // r[impl dashboard.spec.outline-totals]
  // Compute overall coverage totals from root-level entries
  const overallCoverage = useMemo(() => {
    let total = 0;
    let implCount = 0;
    let verifyCount = 0;
    for (const node of outlineTree) {
      total += node.entry.aggregated.total;
      implCount += node.entry.aggregated.implCount;
      verifyCount += node.entry.aggregated.verifyCount;
    }
    const implPct = total > 0 ? Math.round((implCount / total) * 100) : 0;
    const verifyPct = total > 0 ? Math.round((verifyCount / total) * 100) : 0;
    return { total, implCount, verifyCount, implPct, verifyPct };
  }, [outlineTree]);

  // Concatenate all sections' HTML (sections are pre-sorted by weight on server)
  const processedContent = useMemo(() => {
    if (!spec?.sections) return "";
    return spec.sections.map((s) => s.html).join("\n");
  }, [spec?.sections]);

  // Inject head scripts (e.g. mermaid.js) from server-side rendered content.
  // Must use createElement("script") — scripts created via innerHTML do not execute.
  useEffect(() => {
    console.log("[tracey] head_injections:", spec?.head_injections);
    if (!spec?.head_injections?.length) return;
    const injected: HTMLElement[] = [];
    for (const html of spec.head_injections) {
      const key = `tracey-head-${btoa(html).slice(0, 16)}`;
      if (document.getElementById(key)) {
        console.log("[tracey] head injection already present, skipping:", key);
        continue;
      }
      const tmp = document.createElement("div");
      tmp.innerHTML = html;
      const parsed = tmp.firstElementChild;
      if (!parsed) {
        console.warn("[tracey] head injection parsed to nothing:", html);
        continue;
      }
      let el: HTMLElement;
      if (parsed.tagName === "SCRIPT") {
        const script = document.createElement("script");
        for (const attr of parsed.attributes) {
          script.setAttribute(attr.name, attr.value);
        }
        script.textContent = parsed.textContent;
        el = script;
      } else {
        el = parsed as HTMLElement;
      }
      el.id = key;
      console.log("[tracey] injecting into head:", el.outerHTML.slice(0, 120));
      document.head.appendChild(el);
      injected.push(el);
    }
    return () => {
      for (const el of injected) el.remove();
    };
  }, [spec?.head_injections]);

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

  // Custom smooth scroll with controllable duration (shared by j/k and gg/G)
  const smoothScrollTo = (element: HTMLElement, target: number, duration: number) => {
    const start = element.scrollTop;
    const distance = target - start;
    const startTime = performance.now();

    const animate = (currentTime: number) => {
      const elapsed = currentTime - startTime;
      const progress = Math.min(elapsed / duration, 1);
      // Ease out cubic
      const eased = 1 - Math.pow(1 - progress, 3);
      element.scrollTop = start + distance * eased;
      if (progress < 1) {
        requestAnimationFrame(animate);
      }
    };
    requestAnimationFrame(animate);
  };

  // Keyboard navigation: j/k to move between requirements, J/K (shift) for uncovered only
  // r[impl dashboard.editing.keyboard.navigation]
  // r[impl dashboard.editing.keyboard.next-uncovered]
  // r[impl dashboard.editing.keyboard.prev-uncovered]
  useEffect(() => {
    if (!contentRef.current || !contentBodyRef.current) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      // Don't handle if user is typing in an input/textarea or if modifiers are pressed
      const target = e.target as HTMLElement;
      if (
        target.tagName === "INPUT" ||
        target.tagName === "TEXTAREA" ||
        target.isContentEditable ||
        e.ctrlKey ||
        e.metaKey ||
        e.altKey
      ) {
        return;
      }

      if (e.key !== "j" && e.key !== "k" && e.key !== "J" && e.key !== "K" && e.key !== "e") return;

      // Get all requirements, optionally filtered to uncovered only for Shift+J/K
      const uncoveredOnly = e.shiftKey && (e.key === "J" || e.key === "K");
      const selector = uncoveredOnly
        ? ".req-container.req-uncovered[id]"
        : ".req-container[id]";
      const reqs = Array.from(
        contentRef.current?.querySelectorAll(selector) || []
      ) as HTMLElement[];

      if (reqs.length === 0) {
        // No (uncovered) requirements found
        return;
      }

      // Find current requirement from hash or viewport position
      const hash = window.location.hash.slice(1);
      let currentIndex = -1;

      // First try to find by hash
      if (hash) {
        currentIndex = reqs.findIndex((el) => el.id === hash);
      }

      // If not found by hash, find the first visible requirement
      if (currentIndex === -1 && contentBodyRef.current) {
        const scrollTop = contentBodyRef.current.scrollTop;
        const viewportHeight = contentBodyRef.current.clientHeight;
        const viewportMiddle = scrollTop + viewportHeight / 3;

        for (let i = 0; i < reqs.length; i++) {
          const rect = reqs[i].getBoundingClientRect();
          const containerRect = contentBodyRef.current.getBoundingClientRect();
          const elTop = rect.top - containerRect.top + scrollTop;

          if (elTop >= viewportMiddle - 50) {
            currentIndex = Math.max(0, i - 1);
            break;
          }
        }
        // If we scrolled past all, use the last one
        if (currentIndex === -1) {
          currentIndex = reqs.length - 1;
        }
      }

      // Handle 'e' key - open editor for focused requirement
      if (e.key === "e") {
        if (currentIndex === -1) return;
        e.preventDefault();
        const focusedReq = reqs[currentIndex];
        const editBadge = focusedReq.querySelector("button.req-badge.req-edit") as HTMLElement | null;
        if (editBadge) {
          editBadge.click();
        }
        return;
      }

      e.preventDefault();

      // Calculate next index with wrapping
      // j/J = next, k/K = previous
      let nextIndex: number;
      const keyLower = e.key.toLowerCase();
      if (keyLower === "j") {
        // Next requirement, wrap to first
        nextIndex = currentIndex === -1 ? 0 : (currentIndex + 1) % reqs.length;
      } else {
        // Previous requirement, wrap to last
        nextIndex =
          currentIndex === -1
            ? reqs.length - 1
            : (currentIndex - 1 + reqs.length) % reqs.length;
      }

      const targetReq = reqs[nextIndex];
      const reqId = targetReq.id;

      // Update hash and scroll
      history.pushState(null, "", `#${reqId}`);

      // Remove focus class from ALL requirements (not just filtered list), add to target
      const allReqs = contentRef.current?.querySelectorAll(".req-container[id]") || [];
      for (const req of allReqs) {
        req.classList.remove("req-focused");
      }
      targetReq.classList.add("req-focused");

      // Scroll to show the requirement at ~1/3 from top of viewport
      const viewportHeight = contentBodyRef.current?.clientHeight || 0;
      const targetScrollTop = Math.max(0, targetReq.offsetTop - viewportHeight / 3);
      smoothScrollTo(contentBodyRef.current, targetScrollTop, 100);
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [processedContent]);

  // gg/G navigation: gg = top, G = bottom
  // yy/yl yank: yy = copy ID + text, yl = copy ID only
  // r[impl dashboard.editing.keyboard.goto-top]
  // r[impl dashboard.editing.keyboard.goto-bottom]
  // r[impl dashboard.editing.keyboard.yank-full]
  // r[impl dashboard.editing.keyboard.yank-link]
  useEffect(() => {
    let pendingKeyTimeout: number | null = null;

    const clearPending = () => {
      setPendingKey(null);
      if (pendingKeyTimeout) {
        clearTimeout(pendingKeyTimeout);
        pendingKeyTimeout = null;
      }
    };

    const setPending = (key: string) => {
      setPendingKey(key);
      if (pendingKeyTimeout) clearTimeout(pendingKeyTimeout);
      pendingKeyTimeout = window.setTimeout(() => {
        setPendingKey(null);
        pendingKeyTimeout = null;
      }, 1000);
    };

    const showCopiedNotification = (element: HTMLElement) => {
      element.classList.add("req-copy-success");
      setTimeout(() => element.classList.remove("req-copy-success"), 1000);
    };

    const getFocusedRequirement = (): HTMLElement | null => {
      const hash = window.location.hash.slice(1);
      if (hash) {
        const el = contentRef.current?.querySelector(`#${CSS.escape(hash)}`);
        if (el?.classList.contains("req-container")) {
          return el as HTMLElement;
        }
      }
      return contentRef.current?.querySelector(".req-container.req-focused") as HTMLElement | null;
    };

    const handleKeyDown = (e: KeyboardEvent) => {
      // Check refs inside handler - they may not be ready at effect registration
      if (!contentBodyRef.current || !contentRef.current) return;

      const target = e.target as HTMLElement;
      if (
        target.tagName === "INPUT" ||
        target.tagName === "TEXTAREA" ||
        target.isContentEditable ||
        e.ctrlKey ||
        e.metaKey ||
        e.altKey
      ) {
        return;
      }

      // Get current pending key from DOM (state might be stale in closure)
      const currentPending = document.querySelector(".vim-pending-key")?.textContent || null;

      // G (shift+g) = go to bottom and focus last requirement
      if (e.key === "G") {
        e.preventDefault();
        clearPending();
        smoothScrollTo(contentBodyRef.current, contentBodyRef.current.scrollHeight, 150);
        // Focus the last requirement
        const allReqs = contentRef.current.querySelectorAll(".req-container[id]");
        const lastReq = allReqs[allReqs.length - 1] as HTMLElement | undefined;
        if (lastReq) {
          for (const req of allReqs) req.classList.remove("req-focused");
          lastReq.classList.add("req-focused");
          history.replaceState(null, "", `#${lastReq.id}`);
        }
        return;
      }

      // gg = go to top and focus first requirement
      if (e.key === "g" && !e.shiftKey) {
        if (currentPending === "g") {
          e.preventDefault();
          clearPending();
          smoothScrollTo(contentBodyRef.current, 0, 150);
          // Focus the first requirement
          const allReqs = contentRef.current.querySelectorAll(".req-container[id]");
          const firstReq = allReqs[0] as HTMLElement | undefined;
          if (firstReq) {
            for (const req of allReqs) req.classList.remove("req-focused");
            firstReq.classList.add("req-focused");
            history.replaceState(null, "", `#${firstReq.id}`);
          }
        } else {
          e.preventDefault();
          setPending("g");
        }
        return;
      }

      // yl = yank link (copy requirement ID only)
      if (e.key === "l" && currentPending === "y") {
        e.preventDefault();
        clearPending();
        const req = getFocusedRequirement();
        if (req) {
          const reqId = req.id.replace(/^r-/, "");
          navigator.clipboard?.writeText(reqId);
          showCopiedNotification(req);
        }
        return;
      }

      // yy = yank full (copy requirement ID + markdown text)
      if (e.key === "y" && !e.shiftKey) {
        if (currentPending === "y") {
          e.preventDefault();
          clearPending();
          const req = getFocusedRequirement();
          if (req) {
            const reqId = req.id.replace(/^r-/, "");
            const contentEl = req.querySelector(".req-content");
            const text = contentEl?.textContent?.trim() || "";
            const fullText = `r[${reqId}]\n${text}`;
            navigator.clipboard?.writeText(fullText);
            showCopiedNotification(req);
          }
        } else {
          e.preventDefault();
          setPending("y");
        }
        return;
      }

      // Any other key clears pending
      clearPending();
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("keydown", handleKeyDown);
      if (pendingKeyTimeout) clearTimeout(pendingKeyTimeout);
    };
  }, []);

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

    const handleClick = async (e: Event) => {
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

      // r[impl dashboard.editing.copy.button]
      // r[impl dashboard.editing.copy.format]
      // r[impl dashboard.editing.copy.feedback]
      // Handle Copy badge clicks - copy requirement ID to clipboard
      const copyBadge = target.closest("button.req-badge.req-copy") as HTMLElement | null;
      if (copyBadge) {
        e.preventDefault();
        const reqId = copyBadge.dataset.reqId;
        if (reqId) {
          navigator.clipboard
            .writeText(reqId)
            .then(() => {
              // Visual feedback: briefly change button appearance
              const originalText = copyBadge.innerHTML;
              copyBadge.innerHTML =
                '<svg class="req-copy-icon" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"></polyline></svg> Copied!';
              copyBadge.classList.add("req-copy-success");
              setTimeout(() => {
                copyBadge.innerHTML = originalText;
                copyBadge.classList.remove("req-copy-success");
              }, 1500);
            })
            .catch((err) => {
              console.error("Failed to copy:", err);
              alert("Failed to copy requirement ID");
            });
        }
        return;
      }

      // r[impl dashboard.impl-preview.modal]
      // r[impl dashboard.impl-preview.stay-in-spec]
      // Handle impl/test badge clicks - show preview modal instead of navigating
      const implBadge = target.closest(
        "a.req-badge.req-impl, a.req-badge.req-test",
      ) as HTMLAnchorElement | null;
      if (implBadge) {
        console.log("Intercepted impl/test badge click:", implBadge);
        e.preventDefault();
        e.stopPropagation();
        const file = implBadge.dataset.file;
        const line = implBadge.dataset.line ? parseInt(implBadge.dataset.line, 10) : null;
        const type = implBadge.classList.contains("req-impl") ? "impl" : "verify";
        console.log("File:", file, "Line:", line, "Type:", type);
        if (file && line !== null) {
          // Fetch syntax-highlighted file content
          const params = new URLSearchParams({ path: file });
          if (specName) params.append("spec", specName);
          if (selectedImpl) params.append("impl", selectedImpl);
          fetch(`/api/file?${params}`)
            .then((res) => res.json())
            .then((data: FileContent) => {
              console.log("Setting preview modal");
              // r[impl code-unit.nested.smallest]
              // Find the smallest code unit containing this line.
              // Server-side code unit extraction includes preceding comments in startLine,
              // so a simple containment check works correctly.
              const unit = data.units
                .filter((u) => line >= u.startLine && line <= u.endLine)
                .reduce<typeof data.units[0] | null>(
                  (smallest, current) => {
                    if (!smallest) return current;
                    const smallestSize = smallest.endLine - smallest.startLine;
                    const currentSize = current.endLine - current.startLine;
                    return currentSize < smallestSize ? current : smallest;
                  },
                  null
                );
              const lineEnd = unit ? unit.endLine : line;
              setPreviewModal({ fileData: data, line, lineEnd, type });
            })
            .catch((err) => {
              console.error("Failed to fetch file:", err);
              alert("Failed to load file preview");
            });
        }
        return;
      }

      // r[impl dashboard.editing.activation.click]
      // Handle Edit badge clicks - mount inline editor
      const editBadge = target.closest("button.req-badge.req-edit") as HTMLElement | null;
      if (editBadge) {
        e.preventDefault();
        const sourceFile = editBadge.dataset.sourceFile;
        const byteRange = editBadge.dataset.br;
        if (sourceFile && byteRange) {
          // r[impl dashboard.editing.git.check-required]
          // Check if file is in git repository
          try {
            const gitCheckResponse = await fetch(
              `/api/check-git?${new URLSearchParams({ path: sourceFile })}`,
            );
            if (gitCheckResponse.ok) {
              const gitData = await gitCheckResponse.json();
              if (!gitData.in_git) {
                // r[impl dashboard.editing.git.error-message]
                alert(
                  "This file is not in a git repository. Tracey requires git for safe editing.",
                );
                return;
              }
            }
          } catch (err) {
            console.error("Git check failed:", err);
            alert("Failed to verify git status. Please try again.");
            return;
          }

          // Find the req-container
          const reqContainer = editBadge.closest(".req-container") as HTMLElement | null;
          if (reqContainer?.parentElement) {
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
            <span class="legend-item"><span class="legend-dot legend-dot--impl"></span>${overallCoverage.implPct}%</span>
            <span class="legend-item"><span class="legend-dot legend-dot--test"></span>${overallCoverage.verifyPct}%</span>
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
      ${editorState &&
      html`<${MarkdownEditor}
        filePath=${editorState.filePath}
        byteRange=${editorState.byteRange}
        onClose=${() => setEditorState(null)}
      />`}
      ${previewModal &&
      html`<${ImplementationPreviewModal}
        fileData=${previewModal.fileData}
        line=${previewModal.line}
        lineEnd=${previewModal.lineEnd}
        type=${previewModal.type}
        config=${config}
        onClose=${() => setPreviewModal(null)}
        onOpenInSources=${() => {
          onSelectFile(previewModal.fileData.path, previewModal.line);
          setPreviewModal(null);
        }}
      />`}
      ${pendingKey &&
      html`<div class="vim-pending-key">${pendingKey}</div>`}
    </div>
  `;
}
