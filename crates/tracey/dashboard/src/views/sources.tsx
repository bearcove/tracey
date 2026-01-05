// Sources view - file tree with code viewer
import { h } from "preact";
import { useCallback, useEffect, useMemo, useRef, useState } from "preact/hooks";
import type {
  FileContent,
  FileInfo,
  SourcesViewProps,
  TreeNodeWithCoverage,
  FileInfoWithName,
} from "../types";
import { useFile } from "../hooks";
import { EDITORS, LANG_DEVICON_MAP } from "../config";
import { buildFileTree, getStatClass, getCoverageBadge, splitHighlightedHtml } from "../utils";
import { html, FilePath, CoverageArc } from "../main";

// Declare lucide as global
declare const lucide: { createIcons: (opts?: { nodes?: NodeList }) => void };

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
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});

  // Auto-expand folders containing selected file
  useEffect(() => {
    if (selectedFile) {
      const parts = selectedFile.split("/");
      const toExpand: Record<string, boolean> = {};
      let path = "";
      for (let i = 0; i < parts.length - 1; i++) {
        path = path ? `${path}/${parts[i]}` : parts[i];
        toExpand[path] = true;
      }
      setExpanded((prev) => ({ ...prev, ...toExpand }));
    }
  }, [selectedFile]);

  const sortedChildren = useMemo(() => {
    return Object.entries(node.children).sort(([a], [b]) => a.localeCompare(b));
  }, [node.children]);

  const sortedFiles = useMemo(() => {
    return [...node.files].sort((a, b) => a.name.localeCompare(b.name));
  }, [node.files]);

  const toggleFolder = useCallback((path: string) => {
    setExpanded((prev) => ({ ...prev, [path]: !prev[path] }));
  }, []);

  // Filter by search
  const matchesSearch = useCallback(
    (name: string) => {
      if (!search) return true;
      return name.toLowerCase().includes(search.toLowerCase());
    },
    [search],
  );

  return html`
    ${sortedChildren.map(([name, child]) => {
      const path = parentPath ? `${parentPath}/${name}` : name;
      const isExpanded = expanded[path] ?? depth < 1;
      const badge = getCoverageBadge(child.coveredUnits, child.totalUnits);

      return html`
        <div key=${path} class="tree-folder">
          <div class="tree-folder-header" onClick=${() => toggleFolder(path)}>
            <svg
              class="tree-chevron ${isExpanded ? "expanded" : ""}"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
            >
              <path d="M9 18l6-6-6-6" />
            </svg>
            <i data-lucide="folder${isExpanded ? "-open" : ""}"></i>
            <span class="tree-folder-name">${name}</span>
            <span class="tree-badge ${badge.class}">${badge.text}</span>
          </div>
          ${isExpanded &&
          html`
            <div class="tree-folder-content">
              <${FileTree}
                node=${child}
                selectedFile=${selectedFile}
                onSelectFile=${onSelectFile}
                depth=${depth + 1}
                search=${search}
                parentPath=${path}
              />
            </div>
          `}
        </div>
      `;
    })}
    ${sortedFiles
      .filter((f) => matchesSearch(f.name))
      .map((file) => {
        const path = parentPath ? `${parentPath}/${file.name}` : file.name;
        const isSelected = selectedFile === path;
        const badge = getCoverageBadge(file.coveredUnits, file.totalUnits);
        const ext = file.name.split(".").pop() || "";
        const devicon = LANG_DEVICON_MAP[ext];

        return html`
          <div
            key=${path}
            class="tree-file ${isSelected ? "selected" : ""}"
            onClick=${() => onSelectFile(path)}
          >
            ${devicon ? html`<i class="${devicon}"></i>` : html`<i data-lucide="file"></i>`}
            <span class="tree-file-name">${file.name}</span>
            <span class="tree-badge ${badge.class}">${badge.text}</span>
          </div>
        `;
      })}
  `;
}

// Code view component
interface CodeViewProps {
  file: FileContent;
  config: { projectRoot?: string };
  selectedLine: number | null;
  onSelectRule: (ruleId: string) => void;
}

function CodeView({ file, config, selectedLine, onSelectRule }: CodeViewProps) {
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
            const isSelected = selectedLine === lineNum;

            return html`
              <tr
                key=${lineNum}
                class="code-line ${isSelected ? "selected" : ""} ${hasRules ? "has-rules" : ""}"
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
