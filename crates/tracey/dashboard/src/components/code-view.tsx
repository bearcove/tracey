// Code viewer component with syntax highlighting

function CodeView({ file, config, selectedLine, onSelectRule }: CodeViewProps) {
  const rawLines = file.content.split("\n");
  // Use server-side highlighted HTML, splitting into lines with balanced tags
  const highlightedLines = useMemo(() => {
    if (!file.html) return null;
    // arborium wraps in <pre><code>...</code></pre>, extract inner content
    const match = file.html.match(/<pre[^>]*><code[^>]*>([\s\S]*)<\/code><\/pre>/);
    const inner = match ? match[1] : file.html;
    return splitHighlightedHtml(inner);
  }, [file.html]);
  const [popoverLine, setPopoverLine] = useState(null);
  const [highlightedLineNum, setHighlightedLineNum] = useState(null);
  const codeViewRef = useRef(null);

  // Build line annotations
  const lineAnnotations = useMemo(() => {
    const annotations = new Map();
    for (const unit of file.units) {
      for (let line = unit.startLine; line <= unit.endLine; line++) {
        if (!annotations.has(line)) {
          annotations.set(line, { units: [], ruleRefs: new Set() });
        }
        const anno = annotations.get(line);
        anno.units.push(unit);
        for (const ref of unit.ruleRefs) {
          anno.ruleRefs.add(ref);
        }
      }
    }
    return annotations;
  }, [file]);

  // Use highlighted lines if available, otherwise show raw (escaped)
  const displayLines =
    highlightedLines ||
    rawLines.map((line) => line.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;"));

  // Full path for editor URLs
  const fullPath = config?.projectRoot ? `${config.projectRoot}/${file.path}` : file.path;

  // Scroll to selected line when it changes
  useEffect(() => {
    if (selectedLine && codeViewRef.current && displayLines) {
      // Use requestAnimationFrame to ensure DOM is rendered
      requestAnimationFrame(() => {
        const lineElement = codeViewRef.current?.querySelector(`[data-line="${selectedLine}"]`);
        if (lineElement) {
          const container = codeViewRef.current.closest(".content-body");
          if (container) {
            // Calculate position to leave ~5 lines above, plus extra for headers
            const lineHeight = lineElement.offsetHeight;
            const headerOffset = 120; // header + stats bar
            const targetScrollTop = lineElement.offsetTop - lineHeight * 5 - headerOffset;
            container.scrollTo({ top: Math.max(0, targetScrollTop) });
          }
          // Highlight the line (permanent until navigation changes)
          setHighlightedLineNum(selectedLine);
        }
      });
    }
  }, [selectedLine, file.path, displayLines]);

  // Close popover when clicking outside
  useEffect(() => {
    const handleClick = (e) => {
      if (!e.target.closest(".line-popover") && !e.target.closest(".line-number")) {
        setPopoverLine(null);
      }
    };
    document.addEventListener("click", handleClick);
    return () => document.removeEventListener("click", handleClick);
  }, []);

  return html`
    <div class="code-view" ref=${codeViewRef}>
      ${displayLines.map((lineHtml, i) => {
        const lineNum = i + 1;
        const anno = lineAnnotations.get(lineNum);
        const covered = anno && anno.ruleRefs.size > 0;
        const inUnit = anno && anno.units.length > 0;
        const isHighlighted = highlightedLineNum === lineNum;

        return html`
          <div
            key=${lineNum}
            data-line=${lineNum}
            class="code-line ${inUnit ? (covered ? "covered" : "uncovered") : ""} ${isHighlighted
              ? "highlighted"
              : ""}"
          >
            <span
              class="line-number"
              onClick=${(e) => {
                e.stopPropagation();
                setPopoverLine(popoverLine === lineNum ? null : lineNum);
              }}
            >
              ${lineNum}
              ${popoverLine === lineNum &&
              html`
                <div class="line-popover">
                  ${Object.entries(EDITORS).map(
                    ([key, cfg]) => html`
                      <a
                        key=${key}
                        href=${cfg.urlTemplate(fullPath, lineNum)}
                        class="popover-btn"
                        title="Open in ${cfg.name}"
                      >
                        ${cfg.devicon
                          ? html`<i class="${cfg.devicon}"></i>`
                          : html`<span dangerouslySetInnerHTML=${{ __html: cfg.icon }}></span>`}
                        <span>${cfg.name}</span>
                      </a>
                    `,
                  )}
                </div>
              `}
            </span>
            <span class="line-content" dangerouslySetInnerHTML=${{ __html: lineHtml || " " }} />
            ${anno &&
            anno.ruleRefs.size > 0 &&
            html`
              <span class="line-annotations">
                <span class="annotation-count" title=${[...anno.ruleRefs].join(", ")}
                  >${anno.ruleRefs.size}</span
                >
                <span class="annotation-badges">
                  ${[...anno.ruleRefs].map(
                    (ref) => html`
                      <a
                        key=${ref}
                        class="annotation-badge"
                        href=${buildUrl("spec", { rule: ref })}
                        onClick=${(e) => {
                          e.preventDefault();
                          onSelectRule(ref);
                        }}
                        >${ref}</a
                      >
                    `,
                  )}
                </span>
              </span>
            `}
          </div>
        `;
      })}
    </div>
  `;
}
