import { useCallback, useEffect, useMemo, useRef, useState } from "preact/hooks";
import { EDITORS } from "../config";
import { useFile } from "../hooks";
import { FilePath, html, LangIcon } from "../main";
import type { FileContent, SourcesViewProps, TreeNodeWithCoverage } from "../types";
import { buildFileTree, getCoverageBadge, getStatClass, splitHighlightedHtml } from "../utils";

// Declare lucide as global
declare const lucide: { createIcons: (opts?: { nodes?: NodeList }) => void };

// r[impl dashboard.sources.file-tree]
// r[impl dashboard.sources.tree-coverage]

// File tree component
interface FileTreeProps {
  node: TreeNodeWithCoverage;
  selectedFile: string | null;
  onSelectFile: (path: string, line?: number | null, context?: string | null) => void;
  depth?: number;
  search?: string;
  parentPath?: string;
}

function FileTree({
  node,
  selectedFile,
  onSelectFile,
  depth = 0,
  search = "",
  parentPath = "",
}: FileTreeProps) {
  // Check if selected file is in this subtree
  const currentPath = parentPath ? `${parentPath}/${node.name}` : node.name;
  const containsSelectedFile = selectedFile?.startsWith(currentPath + "/");
  const hasSelectedFile =
    selectedFile && (containsSelectedFile || node.files.some((f) => f.path === selectedFile));

  const [open, setOpen] = useState(depth < 2 || !!hasSelectedFile);

  // Auto-expand when selected file changes to be in this subtree
  useEffect(() => {
    if (hasSelectedFile && !open) {
      setOpen(true);
    }
  }, [selectedFile, hasSelectedFile]);

  const folders = Object.values(node.children).sort((a, b) => a.name.localeCompare(b.name));
  const files = node.files.sort((a, b) => a.name.localeCompare(b.name));

  // Filter if searching
  const matchesSearch = (path: string) => {
    if (!search) return true;
    return path.toLowerCase().includes(search.toLowerCase());
  };

  if (depth === 0) {
    return html`
      <div class="file-tree">
        ${folders.map(
          (f) => html`
            <${FileTree}
              key=${f.name}
              node=${f}
              selectedFile=${selectedFile}
              onSelectFile=${onSelectFile}
              depth=${depth + 1}
              search=${search}
              parentPath=""
            />
          `,
        )}
        ${files
          .filter((f) => matchesSearch(f.path))
          .map(
            (f) => html`
              <${FileTreeFile}
                key=${f.path}
                file=${f}
                selected=${selectedFile === f.path}
                onClick=${() => onSelectFile(f.path)}
              />
            `,
          )}
      </div>
    `;
  }

  const hasMatchingFiles =
    files.some((f) => matchesSearch(f.path)) ||
    folders.some(
      (f) => Object.values(f.children).length > 0 || f.files.some((ff) => matchesSearch(ff.path)),
    );

  if (search && !hasMatchingFiles) return null;

  const folderBadge = getCoverageBadge(node.coveredUnits, node.totalUnits);

  return html`
    <div class="tree-folder ${open ? "open" : ""}">
      <div class="tree-folder-header" onClick=${() => setOpen(!open)}>
        <div class="tree-folder-left">
          <svg
            class="tree-folder-icon"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
          >
            <path d="M9 18l6-6-6-6" />
          </svg>
          <span>${node.name}</span>
        </div>
        <span class="folder-badge ${folderBadge.class}">${folderBadge.text}</span>
      </div>
      <div class="tree-folder-children">
        ${folders.map(
          (f) => html`
            <${FileTree}
              key=${f.name}
              node=${f}
              selectedFile=${selectedFile}
              onSelectFile=${onSelectFile}
              depth=${depth + 1}
              search=${search}
              parentPath=${currentPath}
            />
          `,
        )}
        ${files
          .filter((f) => matchesSearch(f.path))
          .map(
            (f) => html`
              <${FileTreeFile}
                key=${f.path}
                file=${f}
                selected=${selectedFile === f.path}
                onClick=${() => onSelectFile(f.path)}
              />
            `,
          )}
      </div>
    </div>
  `;
}

interface FileTreeFileProps {
  file: { name: string; path: string; coveredUnits: number; totalUnits: number };
  selected: boolean;
  onClick: () => void;
}

function FileTreeFile({ file, selected, onClick }: FileTreeFileProps) {
  const badge = getCoverageBadge(file.coveredUnits, file.totalUnits);

  return html`
    <div class="tree-file ${selected ? "selected" : ""}" onClick=${onClick}>
      <${LangIcon} filePath=${file.name} className="tree-file-icon" />
      <span class="tree-file-name">${file.name}</span>
      <span class="tree-file-badge ${badge.class}">${badge.text}</span>
    </div>
  `;
}

// r[impl dashboard.sources.code-view]
// r[impl dashboard.sources.line-numbers]
// r[impl dashboard.sources.line-annotations]
// r[impl dashboard.sources.line-highlight]
// r[impl dashboard.sources.editor-open]
// Code view component
export interface CodeViewProps {
  file: FileContent;
  config: { projectRoot?: string };
  selectedLine: number | null;
  selectedLineEnd?: number | null;
  selectedType?: "impl" | "verify";
  onSelectRule: (ruleId: string) => void;
}

export function CodeView({ file, config, selectedLine, selectedLineEnd, selectedType, onSelectRule }: CodeViewProps) {
  const codeRef = useRef<HTMLDivElement>(null);
  const lines = useMemo(() => splitHighlightedHtml(file.html), [file.html]);

  // Scroll to selected line
  useEffect(() => {
    if (selectedLine && codeRef.current) {
      const lineEl = codeRef.current.querySelector(`[data-line="${selectedLine}"]`);
      if (lineEl) {
        lineEl.scrollIntoView({ block: "center" });
      }
    }
  }, [selectedLine, file.path]);

  // Build line metadata from code units
  const lineMetadata = useMemo(() => {
    const meta: Record<number, { rules: string[]; kind: string | null }> = {};
    for (const unit of file.units) {
      for (let line = unit.startLine; line <= unit.endLine; line++) {
        if (!meta[line]) {
          meta[line] = { rules: [], kind: null };
        }
        meta[line].rules.push(...unit.ruleRefs);
        if (line === unit.startLine) {
          meta[line].kind = unit.kind;
        }
      }
    }
    return meta;
  }, [file.units]);

  const handleLineClick = useCallback(
    (lineNum: number) => {
      const meta = lineMetadata[lineNum];
      if (meta?.rules.length) {
        onSelectRule(meta.rules[0]);
      }
    },
    [lineMetadata, onSelectRule],
  );

  const handleEditorOpen = useCallback(
    (lineNum: number) => {
      const fullPath = config.projectRoot ? `${config.projectRoot}/${file.path}` : file.path;
      console.log("Opening in editor - projectRoot:", config.projectRoot);
      console.log("Opening in editor - file.path:", file.path);
      console.log("Opening in editor - fullPath:", fullPath);
      window.location.href = EDITORS.zed.urlTemplate(fullPath, lineNum);
    },
    [config.projectRoot, file.path],
  );

  return html`
    <div class="code-view" ref=${codeRef}>
      <table class="code-table">
        <tbody>
          ${lines.map((lineHtml, idx) => {
            const lineNum = idx + 1;
            const meta = lineMetadata[lineNum];
            const hasRules = meta?.rules.length > 0;
            const isSelected = selectedLine !== null &&
              lineNum >= selectedLine &&
              lineNum <= (selectedLineEnd ?? selectedLine);
            const selectedClass = isSelected
              ? selectedType === "verify" ? "selected-verify" : "selected-impl"
              : "";

            return html`
              <tr
                key=${lineNum}
                class="code-line ${selectedClass} ${hasRules ? "has-rules" : ""}"
                data-line=${lineNum}
              >
                <td class="line-number" onClick=${() => handleEditorOpen(lineNum)}>${lineNum}</td>
                <td class="line-gutter">
                  ${hasRules &&
                  html`
                    <span
                      class="rule-indicator"
                      title=${meta.rules.join(", ")}
                      onClick=${() => handleLineClick(lineNum)}
                    >
                      <svg viewBox="0 0 24 24" fill="currentColor">
                        <circle cx="12" cy="12" r="4" />
                      </svg>
                    </span>
                  `}
                </td>
                <td class="line-content">
                  <code dangerouslySetInnerHTML=${{ __html: lineHtml || "&nbsp;" }} />
                </td>
              </tr>
            `;
          })}
        </tbody>
      </table>
    </div>
  `;
}

// r[impl dashboard.sources.req-context]
export function SourcesView({
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

  const isActiveRef = useCallback(
    (ref: { file: string; line: number }) => {
      return ref.file === selectedFile && ref.line === selectedLine;
    },
    [selectedFile, selectedLine],
  );

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
              ${/* r[impl dashboard.sources.req-context] */ null}
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
                  ${contextRule.html &&
                  html` <div class="rule-context-text" dangerouslySetInnerHTML=${{ __html: contextRule.html }}></div> `}
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
