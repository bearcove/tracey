// Spec view - displays specification with outline sidebar

function SpecView({
  config,
  version,
  selectedSpec,
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
  const hasMultipleSpecs = (config.specs?.length || 0) > 1;
  const [activeHeading, setActiveHeading] = useState(null);
  const [collapsedSlugs, setCollapsedSlugs] = useState<Set<string>>(() => getCollapsedSlugs());
  const contentRef = useRef(null);
  const contentBodyRef = useRef(null);
  const initialScrollPosition = useRef(scrollPosition);
  const lastScrolledHeading = useRef<string | null>(null);

  // Use outline from API (already has coverage info)
  const outline = spec?.outline || [];

  // Build hierarchical tree from flat outline
  const outlineTree = useMemo(() => buildOutlineTree(outline), [outline]);

  // Toggle collapse state for a heading
  const handleToggleCollapse = useCallback((slug: string) => {
    setCollapsedSlugs((prev) => {
      const next = new Set(prev);
      if (next.has(slug)) {
        next.delete(slug);
      } else {
        next.add(slug);
      }
      saveCollapsedSlugs(next);
      return next;
    });
  }, []);

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

      // Find the heading closest to the top of the viewport (but not past it)
      const scrollTop = contentBody.scrollTop;
      const viewportTop = 100; // offset from top to consider "active"

      let activeId: string | null = null;

      for (const el of headingElements) {
        const htmlEl = el as HTMLElement;
        const offsetTop = htmlEl.offsetTop;

        // If this heading is above the viewport threshold, it's the current section
        if (offsetTop <= scrollTop + viewportTop) {
          activeId = htmlEl.id;
        } else {
          // Once we find a heading below the threshold, stop
          break;
        }
      }

      // If no heading is above threshold, use the first one
      if (!activeId && headingElements.length > 0) {
        activeId = (headingElements[0] as HTMLElement).id;
      }

      if (activeId) {
        setActiveHeading(activeId);
      }
    };

    // Initial update
    const timeoutId = setTimeout(updateActiveHeading, 100);

    // Update on scroll
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
      if (onScrollChange) {
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
          nodes: contentRef.current.querySelectorAll("[data-lucide]"),
        });
      });
    }
  }, [processedContent]);

  // Add pencil edit buttons to paragraphs with data-source-file/data-source-line
  useEffect(() => {
    if (!processedContent || !contentRef.current || !config) return;

    // Find all elements with source location data
    const elements = contentRef.current.querySelectorAll("[data-source-file][data-source-line]");

    for (const el of elements) {
      // Skip if already has edit button
      if (el.querySelector(".para-edit-btn")) continue;

      const sourceFile = el.getAttribute("data-source-file");
      const sourceLine = el.getAttribute("data-source-line");
      if (!sourceFile || !sourceLine) continue;

      const fullPath = config.projectRoot ? `${config.projectRoot}/${sourceFile}` : sourceFile;
      const editUrl = EDITORS.zed.urlTemplate(fullPath, parseInt(sourceLine, 10));

      // Create pencil button
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
      const targetScrollTop = el.offsetTop - 100;
      contentBodyRef.current.scrollTo({ top: Math.max(0, targetScrollTop) });
      setActiveHeading(slug);
    }
  }, []);

  // Handle clicks on headings, rule markers, anchor links, and spec refs in the markdown
  useEffect(() => {
    if (!contentRef.current) return;

    const handleClick = (e) => {
      // Handle heading clicks (copy URL)
      const heading = e.target.closest("h1[id], h2[id], h3[id], h4[id]");
      if (heading) {
        const slug = heading.id;
        const url = `${window.location.origin}${window.location.pathname}#${slug}`;
        navigator.clipboard?.writeText(url);
        // Also navigate to the heading
        history.pushState(null, "", `#${slug}`);
        window.dispatchEvent(new HashChangeEvent("hashchange"));
        return;
      }

      // Handle rule marker clicks
      const ruleMarker = e.target.closest("a.rule-marker[data-rule]");
      if (ruleMarker) {
        e.preventDefault();
        const ruleId = ruleMarker.dataset.rule;
        onSelectRule(ruleId);
        return;
      }

      // Handle rule-id badge clicks - open spec source in editor
      const ruleBadge = e.target.closest(
        "a.rule-badge.rule-id[data-source-file][data-source-line]",
      );
      if (ruleBadge) {
        e.preventDefault();
        const sourceFile = ruleBadge.dataset.sourceFile;
        const sourceLine = parseInt(ruleBadge.dataset.sourceLine, 10);
        if (sourceFile && !Number.isNaN(sourceLine)) {
          const fullPath = config.projectRoot ? `${config.projectRoot}/${sourceFile}` : sourceFile;
          // Open in Zed (default editor)
          window.location.href = EDITORS.zed.urlTemplate(fullPath, sourceLine);
        }
        return;
      }

      // Handle impl/test badge clicks with multiple refs - show popup
      const refBadge = e.target.closest("a.rule-badge[data-all-refs]");
      if (refBadge) {
        const allRefsJson = refBadge.dataset.allRefs;
        if (allRefsJson) {
          try {
            const refs = JSON.parse(allRefsJson);
            if (refs.length > 1) {
              e.preventDefault();
              // Show popup with all refs
              showRefsPopup(e, refs, refBadge, onSelectFile);
              return;
            }
          } catch (err) {
            console.error("Failed to parse refs:", err);
          }
        }
        // Single ref or parse error - fall through to default link behavior
      }

      // Handle spec ref clicks - pass rule context
      const specRef = e.target.closest("a.spec-ref");
      if (specRef) {
        e.preventDefault();
        const file = specRef.dataset.file;
        const line = parseInt(specRef.dataset.line, 10);
        // Find the rule ID from the parent rule-block
        const ruleBlock = specRef.closest(".rule-block");
        const ruleMarker = ruleBlock?.querySelector("a.rule-marker[data-rule]");
        const ruleContext = ruleMarker?.dataset.rule || null;
        onSelectFile(file, line, ruleContext);
        return;
      }

      // Handle other anchor links (internal navigation)
      const anchor = e.target.closest("a[href]");
      if (anchor) {
        const href = anchor.getAttribute("href");
        if (!href) return;

        // Check if it's an internal link (same origin)
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

    // Use requestAnimationFrame to ensure DOM is updated after render
    let cancelled = false;
    requestAnimationFrame(() => {
      if (cancelled) return;
      // Double RAF to ensure layout is complete
      requestAnimationFrame(() => {
        if (cancelled || !contentRef.current || !contentBodyRef.current) return;

        if (selectedRule) {
          // Navigate to specific rule
          const ruleEl = contentRef.current.querySelector(`[data-rule="${selectedRule}"]`);
          if (ruleEl) {
            // Use getBoundingClientRect relative to the scroll container
            const containerRect = contentBodyRef.current.getBoundingClientRect();
            const ruleRect = ruleEl.getBoundingClientRect();
            const currentScroll = contentBodyRef.current.scrollTop;
            const targetScrollTop = currentScroll + (ruleRect.top - containerRect.top) - 150;
            contentBodyRef.current.scrollTo({
              top: Math.max(0, targetScrollTop),
            });

            // Add highlight class
            ruleEl.classList.add("rule-marker-highlighted");

            // Remove highlight after animation
            setTimeout(() => {
              ruleEl.classList.remove("rule-marker-highlighted");
            }, 3000);
          }
        } else if (selectedHeading && selectedHeading !== lastScrolledHeading.current) {
          // Navigate to specific heading
          lastScrolledHeading.current = selectedHeading;
          const headingEl = contentRef.current.querySelector(`[id="${selectedHeading}"]`);
          if (headingEl) {
            const targetScrollTop = headingEl.offsetTop - 100;
            contentBodyRef.current.scrollTo({
              top: Math.max(0, targetScrollTop),
            });
            setActiveHeading(selectedHeading);
          }
        } else if (initialScrollPosition.current > 0) {
          // Restore previous scroll position (only on initial mount)
          contentBodyRef.current.scrollTo({
            top: initialScrollPosition.current,
          });
          initialScrollPosition.current = 0; // Clear so we don't restore again
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
        <div class="sidebar-header">Outline</div>
        <div class="sidebar-content">
          <div class="outline-tree">
            <${OutlineTree}
              nodes=${outlineTree}
              activeHeading=${activeHeading}
              onSelectHeading=${scrollToHeading}
              collapsedSlugs=${collapsedSlugs}
              onToggleCollapse=${handleToggleCollapse}
            />
          </div>
        </div>
      </div>
      <div class="content">
        <div class="content-header">
          ${hasMultipleSpecs
            ? html`
                <div class="spec-switcher">
                  ${config.specs.map(
                    (s) => html`
                      <button
                        class="spec-tab ${s.name === specName ? "active" : ""}"
                        onClick=${() => onSelectSpec(s.name)}
                      >
                        ${s.name}
                      </button>
                    `,
                  )}
                </div>
              `
            : html`${spec.name}${spec.sections.length > 1 ? ` (${spec.sections.length} files)` : ""}`}
        </div>
        <div class="content-body" ref=${contentBodyRef}>
          <div
            class="markdown"
            ref=${contentRef}
            dangerouslySetInnerHTML=${{ __html: processedContent }}
          />
        </div>
      </div>
    </div>
  `;
}
