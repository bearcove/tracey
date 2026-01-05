// Sources view - file tree with code viewer

function SourcesView({
  data,
  forward,
  config,
  search,
  selectedFile,
  selectedLine,
  ruleContext,
  onSelectFile,
  onSelectRule,
  onClearContext,
}: SourcesViewProps) {
  const fileTree = useMemo(() => buildFileTree(data.files), [data.files]);
  const file = useFile(selectedFile);

  // Find the rule data if we have a context
  const contextRule = useMemo(() => {
    if (!ruleContext || !forward) return null;
    for (const spec of forward.specs) {
      const rule = spec.rules.find((r) => r.id === ruleContext);
      if (rule) return rule;
    }
    return null;
  }, [ruleContext, forward]);

  const stats = {
    total: data.totalUnits,
    covered: data.coveredUnits,
    pct: data.totalUnits ? (data.coveredUnits / data.totalUnits) * 100 : 0,
  };

  // Check if a ref matches the current file:line
  const isActiveRef = useCallback(
    (ref) => {
      return ref.file === selectedFile && ref.line === selectedLine;
    },
    [selectedFile, selectedLine],
  );

  // Icons
  const closeIcon = html`<svg
    width="14"
    height="14"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    stroke-width="2"
  >
    <path d="M18 6L6 18M6 6l12 12" />
  </svg>`;
  const backIcon = html`<svg
    width="14"
    height="14"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    stroke-width="2"
  >
    <path d="M19 12H5M12 19l-7-7 7-7" />
  </svg>`;

  return html`
    <div class="stats-bar">
      <div class="stat">
        <span class="stat-label">Code Units</span>
        <span class="stat-value">${stats.total}</span>
      </div>
      <div class="stat">
        <span class="stat-label">Spec Coverage</span>
        <span class="stat-value ${getStatClass(stats.pct)}">${stats.pct.toFixed(1)}%</span>
      </div>
      <div class="stat">
        <span class="stat-label">Covered</span>
        <span class="stat-value good">${stats.covered}</span>
      </div>
      <div class="stat">
        <span class="stat-label">Uncovered</span>
        <span class="stat-value ${stats.total - stats.covered > 0 ? "bad" : "good"}"
          >${stats.total - stats.covered}</span
        >
      </div>
    </div>
    <div class="main">
      <div class="sidebar">
        ${contextRule
          ? html`
              <!-- Rule context panel -->
              <div class="rule-context">
                <div class="rule-context-header">
                  <span class="rule-context-id">${contextRule.id}</span>
                  <button
                    class="rule-context-close"
                    onClick=${onClearContext}
                    title="Close context"
                  >
                    ${closeIcon}
                  </button>
                </div>
                <div class="rule-context-body">
                  ${contextRule.text &&
                  html` <div class="rule-context-text">${contextRule.text}</div> `}
                  <div class="rule-context-refs">
                    ${contextRule.implRefs.map(
                      (ref) => html`
                        <div
                          key=${`impl:${ref.file}:${ref.line}`}
                          class="rule-context-ref ${isActiveRef(ref) ? "active" : ""}"
                          onClick=${() => onSelectFile(ref.file, ref.line, ruleContext)}
                          title=${ref.file}
                        >
                          <${FilePath} file=${ref.file} line=${ref.line} short type="impl" />
                        </div>
                      `,
                    )}
                    ${contextRule.verifyRefs.map(
                      (ref) => html`
                        <div
                          key=${`verify:${ref.file}:${ref.line}`}
                          class="rule-context-ref ${isActiveRef(ref) ? "active" : ""}"
                          onClick=${() => onSelectFile(ref.file, ref.line, ruleContext)}
                          title=${ref.file}
                        >
                          <${FilePath} file=${ref.file} line=${ref.line} short type="verify" />
                        </div>
                      `,
                    )}
                  </div>
                  <a class="rule-context-back" onClick=${() => onSelectRule(ruleContext)}>
                    ${backIcon}
                    <span>Back to rule in spec</span>
                  </a>
                </div>
              </div>
            `
          : html`
              <!-- Normal file tree -->
              <div class="sidebar-header">Files</div>
              <div class="sidebar-content">
                <${FileTree}
                  node=${fileTree}
                  selectedFile=${selectedFile}
                  onSelectFile=${onSelectFile}
                  search=${search}
                />
              </div>
            `}
      </div>
      <div class="content">
        ${file
          ? html`
              <div class="content-header">${file.path}</div>
              <div class="content-body">
                <${CodeView}
                  file=${file}
                  config=${config}
                  selectedLine=${selectedLine}
                  onSelectRule=${onSelectRule}
                />
              </div>
            `
          : html` <div class="empty-state">Select a file to view coverage</div> `}
      </div>
    </div>
  `;
}
