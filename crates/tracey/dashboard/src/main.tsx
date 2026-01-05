import htm from "htm";
import { h, render } from "preact";
import { useCallback, useEffect, useMemo, useRef, useState } from "preact/hooks";
// Note: Server-side rendering via bearmark (markdown) and arborium (syntax highlighting)
import "./style.css";
import type {
  ApiData,
  CodeViewProps,
  Config,
  CoverageViewProps,
  Editor,
  FileContent,
  FileInfo,
  FilePathProps,
  FileRefProps,
  FileTreeFileProps,
  FileTreeProps,
  ForwardData,
  HeaderProps,
  LangIconProps,
  LucideIconProps,
  OutlineEntry,
  ReverseData,
  Route,
  SearchModalProps,
  SearchResultItemProps,
  SourcesViewProps,
  SpecContent,
  SpecViewProps,
  TreeNodeWithCoverage,
  ViewType,
} from "./types";

const html = htm.bind(h);

// ========================================================================
// API
// ========================================================================

async function fetchJson<T>(url: string): Promise<T> {
  const res = await fetch(url);
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();
}

// ========================================================================
// Routing
// ========================================================================

function parseRoute(): Route {
  const path = window.location.pathname;
  const params = new URLSearchParams(window.location.search);

  // /sources or /sources/path/to/file.rs:123
  if (path === "/sources" || path.startsWith("/sources/")) {
    const rest = path.length > 9 ? path.slice(9) : ""; // Remove '/sources/'
    const context = params.get("context"); // rule ID context
    if (rest) {
      const colonIdx = rest.lastIndexOf(":");
      if (colonIdx !== -1) {
        const file = rest.slice(0, colonIdx);
        const line = parseInt(rest.slice(colonIdx + 1), 10);
        return {
          view: "sources",
          file,
          line: Number.isNaN(line) ? null : line,
          context,
        };
      }
      return { view: "sources", file: rest, line: null, context };
    }
    return { view: "sources", file: null, line: null, context };
  }
  // /spec or /spec/section/ (also handle / -> /spec)
  if (path === "/" || path.startsWith("/spec")) {
    // Extract path segment after /spec/ (e.g., /spec/data-model/ -> data-model)
    const pathSegment = path.length > 5 ? path.slice(6).replace(/\/$/, "") : null;
    const hashHeading = window.location.hash ? window.location.hash.slice(1) : null;
    // Path segment becomes heading if present, otherwise use hash
    const heading = pathSegment || hashHeading;
    // Rule and spec from query params
    const rule = params.get("rule");
    const spec = params.get("spec");
    return { view: "spec", spec: spec ?? null, rule: rule ?? null, heading };
  }
  // /coverage
  return {
    view: "coverage",
    filter: params.get("filter"), // 'impl' or 'verify' or null
    level: params.get("level"), // 'must', 'should', 'may', or null (all)
  };
}

interface UrlParams {
  file?: string | null;
  line?: number | null;
  context?: string | null;
  spec?: string | null;
  rule?: string | null;
  heading?: string | null;
  filter?: string | null;
  level?: string | null;
}

function buildUrl(view: ViewType, params: UrlParams = {}): string {
  if (view === "sources") {
    const { file, line, context } = params;
    let url = "/sources";
    if (file) {
      url = line ? `/sources/${file}:${line}` : `/sources/${file}`;
    }
    if (context) {
      url += `?context=${encodeURIComponent(context)}`;
    }
    return url;
  }
  if (view === "spec") {
    const { spec, rule, heading } = params;
    const searchParams = new URLSearchParams();
    if (spec) searchParams.set("spec", spec);
    if (rule) searchParams.set("rule", rule);
    const query = searchParams.toString();
    if (heading) return `/spec/${heading}${query ? `?${query}` : ""}`;
    return `/spec${query ? `?${query}` : ""}`;
  }
  // coverage
  const searchParams = new URLSearchParams();
  if (params.filter) searchParams.set("filter", params.filter);
  if (params.level && params.level !== "all") searchParams.set("level", params.level);
  const query = searchParams.toString();
  return `/coverage${query ? `?${query}` : ""}`;
}

function navigate(view: ViewType, params: UrlParams = {}, replace = false): void {
  const url = buildUrl(view, params);
  if (replace) {
    history.replaceState(null, "", url);
  } else {
    history.pushState(null, "", url);
  }
  window.dispatchEvent(new PopStateEvent("popstate"));
}

function useRouter(): Route {
  const [route, setRoute] = useState<Route>(parseRoute);

  useEffect(() => {
    const handleChange = () => setRoute(parseRoute());
    window.addEventListener("popstate", handleChange);
    window.addEventListener("hashchange", handleChange);
    return () => {
      window.removeEventListener("popstate", handleChange);
      window.removeEventListener("hashchange", handleChange);
    };
  }, []);

  return route;
}

// ========================================================================
// Hooks
// ========================================================================

interface UseApiResult {
  data: ApiData | null;
  error: string | null;
  version: string | null;
  refetch: () => Promise<void>;
}

function useApi(): UseApiResult {
  const [data, setData] = useState<ApiData | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [version, setVersion] = useState<string | null>(null);

  const fetchData = useCallback(async () => {
    try {
      const [config, forward, reverse] = await Promise.all([
        fetchJson<Config>("/api/config"),
        fetchJson<ForwardData>("/api/forward"),
        fetchJson<ReverseData>("/api/reverse"),
      ]);
      setData({ config, forward, reverse });
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, []);

  // Initial fetch
  useEffect(() => {
    fetchData();
  }, [fetchData]);

  // Poll for version changes and refetch if changed
  useEffect(() => {
    let active = true;
    let lastVersion: string | null = null;

    async function poll() {
      if (!active) return;
      try {
        const res = await fetchJson<{ version: string }>("/api/version");
        if (lastVersion !== null && res.version !== lastVersion) {
          console.log(`Version changed: ${lastVersion} -> ${res.version}, refetching...`);
          await fetchData();
        }
        lastVersion = res.version;
        setVersion(res.version);
      } catch (e) {
        console.warn("Version poll failed:", e);
      }
      if (active) setTimeout(poll, 500);
    }

    poll();
    return () => {
      active = false;
    };
  }, [fetchData]);

  return { data, error, version, refetch: fetchData };
}

function useFile(path: string | null): FileContent | null {
  const [file, setFile] = useState<FileContent | null>(null);

  useEffect(() => {
    if (!path) {
      setFile(null);
      return;
    }
    fetchJson<FileContent>(`/api/file?path=${encodeURIComponent(path)}`)
      .then(setFile)
      .catch((e) => {
        console.error("Failed to load file:", e);
        setFile(null);
      });
  }, [path]);

  return file;
}

function useSpec(name: string | null, version: string | null): SpecContent | null {
  const [spec, setSpec] = useState<SpecContent | null>(null);

  useEffect(() => {
    if (!name) {
      setSpec(null);
      return;
    }
    fetchJson<SpecContent>(`/api/spec?name=${encodeURIComponent(name)}`)
      .then(setSpec)
      .catch((e) => {
        console.error("Failed to load spec:", e);
        setSpec(null);
      });
  }, [name, version]);

  return spec;
}

// ========================================================================
// Utils
// ========================================================================

function buildFileTree(files: FileInfo[]): TreeNodeWithCoverage {
  const root: TreeNodeWithCoverage = {
    name: "",
    children: {},
    files: [],
    totalUnits: 0,
    coveredUnits: 0,
  };

  for (const file of files) {
    const parts = file.path.split("/");
    let current = root;

    for (let i = 0; i < parts.length - 1; i++) {
      const part = parts[i];
      if (!current.children[part]) {
        current.children[part] = {
          name: part,
          children: {},
          files: [],
          totalUnits: 0,
          coveredUnits: 0,
        };
      }
      current = current.children[part];
    }

    current.files.push({ ...file, name: parts[parts.length - 1] });
  }

  // Compute folder coverage recursively
  function computeCoverage(node: TreeNodeWithCoverage): void {
    let total = 0;
    let covered = 0;

    // Add files in this folder
    for (const f of node.files) {
      total += f.totalUnits || 0;
      covered += f.coveredUnits || 0;
    }

    // Add children folders
    for (const child of Object.values(node.children)) {
      computeCoverage(child);
      total += child.totalUnits;
      covered += child.coveredUnits;
    }

    node.totalUnits = total;
    node.coveredUnits = covered;
  }

  computeCoverage(root);
  return root;
}

function getCoverageBadge(covered: number, total: number): { class: string; text: string } {
  if (total === 0) return { class: "none", text: "-" };
  const pct = (covered / total) * 100;
  if (pct === 100) return { class: "full", text: "100%" };
  if (pct >= 50) return { class: "partial", text: `${Math.round(pct)}%` };
  return { class: "none", text: `${Math.round(pct)}%` };
}

function getStatClass(pct: number): string {
  if (pct >= 80) return "good";
  if (pct >= 50) return "warn";
  return "bad";
}

// Render rule text with backticks -> <code> and RFC 2119 keywords highlighted
function renderRuleText(text: string | undefined): string {
  if (!text) return "";

  // Escape HTML first
  let result = text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");

  // Process `code` (backticks)
  let inCode = false;
  let processed = "";
  for (const char of result) {
    if (char === "`") {
      if (inCode) {
        processed += "</code>";
        inCode = false;
      } else {
        processed += "<code>";
        inCode = true;
      }
    } else {
      processed += char;
    }
  }
  if (inCode) processed += "</code>";
  result = processed;

  // Wrap RFC 2119 keywords (order matters - longer phrases first)
  result = result
    .replace(/\bMUST NOT\b/g, "<kw-must-not>MUST NOT</kw-must-not>")
    .replace(/\bSHALL NOT\b/g, "<kw-shall-not>SHALL NOT</kw-shall-not>")
    .replace(/\bSHOULD NOT\b/g, "<kw-should-not>SHOULD NOT</kw-should-not>")
    .replace(/\bNOT RECOMMENDED\b/g, "<kw-not-recommended>NOT RECOMMENDED</kw-not-recommended>")
    .replace(/\bMUST\b/g, "<kw-must>MUST</kw-must>")
    .replace(/\bREQUIRED\b/g, "<kw-required>REQUIRED</kw-required>")
    .replace(/\bSHALL\b/g, "<kw-shall>SHALL</kw-shall>")
    .replace(/\bSHOULD\b/g, "<kw-should>SHOULD</kw-should>")
    .replace(/\bRECOMMENDED\b/g, "<kw-recommended>RECOMMENDED</kw-recommended>")
    .replace(/\bMAY\b/g, "<kw-may>MAY</kw-may>")
    .replace(/\bOPTIONAL\b/g, "<kw-optional>OPTIONAL</kw-optional>");

  return result;
}

// Split highlighted HTML into self-contained lines
// Each line will have properly balanced open/close tags
function splitHighlightedHtml(html: string) {
  // Use DOMParser for robust HTML parsing
  const parser = new DOMParser();
  const doc = parser.parseFromString(`<div>${html}</div>`, "text/html");
  const container = doc.body.firstChild;

  const lines = [];
  let currentLine = "";
  const openTags = []; // Stack of {tag, attrs}

  function processNode(node) {
    if (node.nodeType === Node.TEXT_NODE) {
      const text = node.textContent;
      for (const char of text) {
        if (char === "\n") {
          // Close tags, push line, reopen tags
          for (let j = openTags.length - 1; j >= 0; j--) {
            currentLine += `</${openTags[j].tag}>`;
          }
          lines.push(currentLine);
          currentLine = "";
          for (const t of openTags) {
            currentLine += `<${t.tag}${t.attrs}>`;
          }
        } else {
          currentLine +=
            char === "<" ? "&lt;" : char === ">" ? "&gt;" : char === "&" ? "&amp;" : char;
        }
      }
    } else if (node.nodeType === Node.ELEMENT_NODE) {
      const tag = node.tagName.toLowerCase();
      let attrs = "";
      for (const attr of node.attributes) {
        attrs += ` ${attr.name}="${attr.value.replace(/"/g, "&quot;")}"`;
      }

      currentLine += `<${tag}${attrs}>`;
      openTags.push({ tag, attrs });

      for (const child of node.childNodes) {
        processNode(child);
      }

      openTags.pop();
      currentLine += `</${tag}>`;
    }
  }

  for (const child of container.childNodes) {
    processNode(child);
  }

  // Push final line if any content remains
  if (currentLine) {
    lines.push(currentLine);
  }

  return lines;
}

// ========================================================================
// Components
// ========================================================================

// Detect platform for keyboard shortcuts
const isMac =
  typeof navigator !== "undefined" && navigator.platform.toUpperCase().indexOf("MAC") >= 0;
const modKey = isMac ? "⌘" : "Ctrl";

function App() {
  const { data, error, version } = useApi();
  const route = useRouter();
  const [search, setSearch] = useState("");
  const [scrollPositions, setScrollPositions] = useState<Record<string, number>>({});
  const [searchOpen, setSearchOpen] = useState(false);

  if (error) return html`<div class="loading">Error: ${error}</div>`;
  if (!data) return html`<div class="loading">Loading...</div>`;

  const { config, forward, reverse } = data;
  const view = route.view;
  const file = route.view === "sources" ? route.file : null;
  const line = route.view === "sources" ? route.line : null;
  const context = route.view === "sources" ? route.context : null;
  const spec = route.view === "spec" ? route.spec : null;
  const rule = route.view === "spec" ? route.rule : null;
  const heading = route.view === "spec" ? route.heading : null;
  const filter = route.view === "coverage" ? route.filter : null;
  const routeLevel = route.view === "coverage" ? route.level : null;

  // Level comes from URL, defaults to 'all'
  const level = routeLevel || "all";

  const handleLevelChange = useCallback(
    (newLevel: string) => {
      navigate("coverage", { filter, level: newLevel }, false);
    },
    [filter],
  );

  const handleViewChange = useCallback((newView) => {
    navigate(newView, {}, false);
  }, []);

  const handleSelectFile = useCallback((filePath, lineNum = null, ruleContext = null) => {
    navigate("sources", { file: filePath, line: lineNum, context: ruleContext }, false);
  }, []);

  const handleSelectSpec = useCallback(
    (specName: string) => {
      navigate("spec", { spec: specName, heading }, false);
    },
    [heading],
  );

  const handleSelectRule = useCallback(
    (ruleId) => {
      navigate("spec", { spec, rule: ruleId }, false);
    },
    [spec],
  );

  const handleClearContext = useCallback(() => {
    navigate("sources", { file, line, context: null }, true);
  }, [file, line]);

  // Global keyboard shortcut for search
  useEffect(() => {
    const handleKeyDown = (e) => {
      if ((e.metaKey || e.ctrlKey) && e.key === "k") {
        e.preventDefault();
        setSearchOpen(true);
      }
      if (e.key === "Escape") {
        setSearchOpen(false);
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, []);

  const handleSearchSelect = useCallback((result) => {
    setSearchOpen(false);
    if (result.kind === "rule") {
      navigate("spec", { rule: result.id }, false);
    } else {
      navigate("sources", { file: result.id, line: result.line }, false);
    }
  }, []);

  const handleFilterChange = useCallback(
    (newFilter: string) => {
      navigate("coverage", { filter: newFilter, level }, false);
    },
    [level],
  );

  return html`
    <div class="layout">
      <${Header}
        view=${view}
        onViewChange=${handleViewChange}
        onOpenSearch=${() => setSearchOpen(true)}
      />

      ${searchOpen &&
      html`
        <${SearchModal} onClose=${() => setSearchOpen(false)} onSelect=${handleSearchSelect} />
      `}
      ${view === "coverage" &&
      html`
        <${CoverageView}
          data=${forward}
          config=${config}
          search=${search}
          onSearchChange=${setSearch}
          level=${level}
          onLevelChange=${handleLevelChange}
          filter=${filter}
          onFilterChange=${handleFilterChange}
          onSelectRule=${handleSelectRule}
          onSelectFile=${handleSelectFile}
        />
      `}
      ${view === "sources" &&
      html`
        <${SourcesView}
          data=${reverse}
          forward=${forward}
          config=${config}
          search=${search}
          onSearchChange=${setSearch}
          selectedFile=${file}
          selectedLine=${line}
          ruleContext=${context}
          onSelectFile=${handleSelectFile}
          onSelectRule=${handleSelectRule}
          onClearContext=${handleClearContext}
        />
      `}
      ${view === "spec" &&
      html`
        <${SpecView}
          config=${config}
          forward=${forward}
          version=${version}
          selectedSpec=${spec}
          selectedRule=${rule}
          selectedHeading=${heading}
          onSelectSpec=${handleSelectSpec}
          onSelectRule=${handleSelectRule}
          onSelectFile=${handleSelectFile}
          scrollPosition=${scrollPositions.spec || 0}
          onScrollChange=${(pos) => setScrollPositions((prev) => ({ ...prev, spec: pos }))}
        />
      `}
    </div>
  `;
}

// Editor configurations with devicon classes (zed uses inline SVG since devicon font doesn't have it yet)
const ZED_SVG = `<svg class="editor-icon-svg" viewBox="0 0 128 128"><path fill="currentColor" d="M12 8a4 4 0 0 0-4 4v88H0V12C0 5.373 5.373 0 12 0h107.172c5.345 0 8.022 6.463 4.242 10.243L57.407 76.25H76V68h8v10.028a4 4 0 0 1-4 4H49.97l-13.727 13.729H98V56h8v47.757a8 8 0 0 1-8 8H27.657l-13.97 13.97H116a4 4 0 0 0 4-4V28h8v93.757c0 6.627-5.373 12-12 12H8.828c-5.345 0-8.022-6.463-4.242-10.243L70.343 57.757H52v8h-8V55.728a4 4 0 0 1 4-4h30.086l13.727-13.728H30V78h-8V30.243a8 8 0 0 1 8-8h70.343l13.97-13.971H12z"/></svg>`;
const EDITORS: Record<string, Editor> = {
  zed: {
    name: "Zed",
    urlTemplate: (path, line) => `zed://file/${path}:${line}`,
    icon: ZED_SVG,
  },
  vscode: {
    name: "VS Code",
    urlTemplate: (path, line) => `vscode://file/${path}:${line}`,
    devicon: "devicon-vscode-plain",
  },
  idea: {
    name: "IntelliJ",
    urlTemplate: (path, line) => `idea://open?file=${path}&line=${line}`,
    devicon: "devicon-intellij-plain",
  },
  vim: {
    name: "Vim",
    urlTemplate: (path, line) => `mvim://open?url=file://${path}&line=${line}`,
    devicon: "devicon-vim-plain",
  },
  neovim: {
    name: "Neovim",
    urlTemplate: (path, line) => `nvim://open?file=${path}&line=${line}`,
    devicon: "devicon-neovim-plain",
  },
  emacs: {
    name: "Emacs",
    urlTemplate: (path, line) => `emacs://open?url=file://${path}&line=${line}`,
    devicon: "devicon-emacs-original",
  },
};

const LEVELS = {
  all: { name: "All", dotClass: "level-dot-all" },
  must: { name: "MUST", dotClass: "level-dot-must" },
  should: { name: "SHOULD", dotClass: "level-dot-should" },
  may: { name: "MAY", dotClass: "level-dot-may" },
};

// SVG arc indicator for coverage progress
// Two-layer design: thin background ring (always 360°) + thick progress arc + centered text
interface CoverageArcProps {
  count: number; // numerator
  total: number; // denominator
  color: string; // stroke color for progress
  title?: string;
  size?: number;
}

// Tab icon names (Lucide)
const TAB_ICON_NAMES = {
  specification: "file-text",
  coverage: "bar-chart-3",
  sources: "folder-open",
};

// Helper to split file path into dir and filename
function splitPath(filePath) {
  const lastSlash = filePath.lastIndexOf("/");
  if (lastSlash === -1) return { dir: "", name: filePath };
  return {
    dir: filePath.slice(0, lastSlash + 1),
    name: filePath.slice(lastSlash + 1),
  };
}

// File reference component (wrapper for backwards compatibility in coverage table)
function FileRef({ file, line, type, onSelectFile }: FileRefProps) {
  return html`
    <div class="ref-line">
      <${FilePath}
        file=${file}
        line=${line}
        type=${type}
        onClick=${() => onSelectFile(file, line)}
      />
    </div>
  `;
}

// Show a popup with all references when clicking a badge with +N
function showRefsPopup(e, refs, badgeElement, onSelectFile) {
  // Remove any existing popup
  const existing = document.querySelector(".refs-popup");
  if (existing) existing.remove();

  // Create popup
  const popup = document.createElement("div");
  popup.className = "refs-popup";

  // Position near the clicked badge
  const rect = badgeElement.getBoundingClientRect();
  popup.style.position = "fixed";
  popup.style.top = `${rect.bottom + 8}px`;
  popup.style.left = `${rect.left}px`;
  popup.style.zIndex = "10000";

  // Build popup content
  const items = refs
    .map((ref) => {
      const filename = ref.file.split("/").pop();
      return `<div class="refs-popup-item" data-file="${ref.file}" data-line="${ref.line}">
        <span class="refs-popup-file">${filename}:${ref.line}</span>
      </div>`;
    })
    .join("");

  popup.innerHTML = `<div class="refs-popup-inner">${items}</div>`;

  // Add click handlers
  popup.addEventListener("click", (e) => {
    const item = (e.target as HTMLElement).closest(".refs-popup-item") as HTMLElement | null;
    if (item) {
      const file = item.dataset.file;
      const line = parseInt(item.dataset.line || "0", 10);
      onSelectFile(file, line);
      popup.remove();
    }
  });

  // Close on outside click
  const closeHandler = (e) => {
    if (!popup.contains(e.target) && !badgeElement.contains(e.target)) {
      popup.remove();
      document.removeEventListener("click", closeHandler);
    }
  };
  setTimeout(() => document.addEventListener("click", closeHandler), 0);

  document.body.appendChild(popup);
}

// ========================================================================
// Mount
// ========================================================================

render(html`<${App} />`, document.getElementById("app"));

// Global keyboard shortcuts
document.addEventListener("keydown", (e) => {
  if ((e.metaKey || e.ctrlKey) && e.key === "k") {
    e.preventDefault();
    (document.querySelector(".search-input") as HTMLElement | null)?.focus();
  }
});
