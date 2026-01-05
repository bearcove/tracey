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

// Declare lucide as global (loaded via CDN)
declare const lucide: { createIcons: (opts?: { nodes?: Node[] }) => void };

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
    // Rule only from query param now
    const rule = params.get("rule");
    return { view: "spec", rule: rule ?? null, heading };
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
    const { rule, heading } = params;
    if (rule) return `/spec?rule=${encodeURIComponent(rule)}`;
    if (heading) return `/spec/${heading}`;
    return "/spec";
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

  const handleSelectRule = useCallback((ruleId) => {
    navigate("spec", { rule: ruleId }, false);
  }, []);

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
          selectedRule=${rule}
          selectedHeading=${heading}
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

// Map file extensions to devicon class names
// See https://devicon.dev/ for available icons
const LANG_DEVICON_MAP = {
  rs: "devicon-rust-original",
  ts: "devicon-typescript-plain",
  tsx: "devicon-typescript-plain",
  js: "devicon-javascript-plain",
  jsx: "devicon-javascript-plain",
  py: "devicon-python-plain",
  go: "devicon-go-plain",
  c: "devicon-c-plain",
  cpp: "devicon-cplusplus-plain",
  h: "devicon-c-plain",
  hpp: "devicon-cplusplus-plain",
  swift: "devicon-swift-plain",
  java: "devicon-java-plain",
  rb: "devicon-ruby-plain",
  md: "devicon-markdown-original",
  json: "devicon-json-plain",
  yaml: "devicon-yaml-plain",
  yml: "devicon-yaml-plain",
  toml: "devicon-toml-plain",
  html: "devicon-html5-plain",
  css: "devicon-css3-plain",
  scss: "devicon-sass-original",
  sass: "devicon-sass-original",
  sh: "devicon-bash-plain",
  bash: "devicon-bash-plain",
  zsh: "devicon-bash-plain",
  sql: "devicon-postgresql-plain",
  kt: "devicon-kotlin-plain",
  scala: "devicon-scala-plain",
  hs: "devicon-haskell-plain",
  ex: "devicon-elixir-plain",
  exs: "devicon-elixir-plain",
  erl: "devicon-erlang-plain",
  clj: "devicon-clojure-plain",
  php: "devicon-php-plain",
  lua: "devicon-lua-plain",
  r: "devicon-r-plain",
  jl: "devicon-julia-plain",
  dart: "devicon-dart-plain",
  vue: "devicon-vuejs-plain",
  svelte: "devicon-svelte-plain",
  // Default fallback - use Lucide file icon
  default: null,
};

// Get devicon class for a file extension (returns null if no devicon available)
function getDeviconClass(filePath) {
  const ext = filePath.split(".").pop()?.toLowerCase();
  return LANG_DEVICON_MAP[ext] || LANG_DEVICON_MAP.default;
}

// Language icon component - uses devicon if available, falls back to Lucide
function LangIcon({ filePath, className = "" }: LangIconProps) {
  const deviconClass = getDeviconClass(filePath);
  const iconRef = useRef(null);

  // For Lucide fallback
  useEffect(() => {
    if (!deviconClass && iconRef.current && typeof lucide !== "undefined") {
      iconRef.current.innerHTML = "";
      const i = document.createElement("i");
      i.setAttribute("data-lucide", "file");
      iconRef.current.appendChild(i);
      lucide.createIcons({ nodes: [i] });
    }
  }, [deviconClass]);

  if (deviconClass) {
    return html`<i class="${deviconClass} ${className}"></i>`;
  }
  return html`<span ref=${iconRef} class=${className}></span>`;
}

// Create a Lucide icon element (for use in htm templates)
function LucideIcon({ name, className = "" }: LucideIconProps) {
  const iconRef = useRef(null);

  useEffect(() => {
    if (iconRef.current && typeof lucide !== "undefined") {
      iconRef.current.innerHTML = "";
      const i = document.createElement("i");
      i.setAttribute("data-lucide", name);
      iconRef.current.appendChild(i);
      lucide.createIcons({ nodes: [i] });
    }
  }, [name]);

  return html`<span ref=${iconRef} class=${className}></span>`;
}

// SVG arc indicator for coverage progress
// Two-layer design: thin background ring (always 360°) + thick progress arc + centered text
interface CoverageArcProps {
  count: number; // numerator
  total: number; // denominator
  color: string; // stroke color for progress
  title?: string;
  size?: number;
}

function CoverageArc({ count, total, color, title, size = 28 }: CoverageArcProps) {
  const bgStrokeWidth = 1.5;
  const fgStrokeWidth = 3;
  const radius = (size - fgStrokeWidth) / 2;
  const center = size / 2;
  const fraction = total > 0 ? count / total : 0;

  // Calculate arc path
  // Start at top (12 o'clock position), draw clockwise
  const startAngle = -Math.PI / 2;
  const endAngle = startAngle + fraction * 2 * Math.PI;

  // Calculate arc endpoints
  const startX = center + radius * Math.cos(startAngle);
  const startY = center + radius * Math.sin(startAngle);
  const endX = center + radius * Math.cos(endAngle);
  const endY = center + radius * Math.sin(endAngle);

  // Large arc flag: 1 if arc > 180 degrees
  const largeArcFlag = fraction > 0.5 ? 1 : 0;

  // SVG arc path
  const arcPath =
    fraction >= 1
      ? null // Use circle for full
      : fraction > 0
        ? `M ${startX} ${startY} A ${radius} ${radius} 0 ${largeArcFlag} 1 ${endX} ${endY}`
        : null;

  return html`
    <svg
      width=${size}
      height=${size}
      viewBox="0 0 ${size} ${size}"
      class="coverage-arc"
      style="flex-shrink: 0"
    >
      <title>${title}</title>
      <!-- Thin background ring (always full circle) -->
      <circle
        cx=${center}
        cy=${center}
        r=${radius}
        fill="none"
        stroke="var(--border)"
        stroke-width=${bgStrokeWidth}
      />
      <!-- Thick progress arc -->
      ${fraction >= 1
        ? html`
            <circle
              cx=${center}
              cy=${center}
              r=${radius}
              fill="none"
              stroke=${color}
              stroke-width=${fgStrokeWidth}
            />
          `
        : arcPath
          ? html`
              <path
                d=${arcPath}
                fill="none"
                stroke=${color}
                stroke-width=${fgStrokeWidth}
                stroke-linecap="round"
              />
            `
          : null}
      <!-- Centered text -->
      <text
        x=${center}
        y=${center}
        text-anchor="middle"
        dominant-baseline="central"
        fill="var(--fg-muted)"
        font-size="8"
        font-weight="500"
      >
        ${count}/${total}
      </text>
    </svg>
  `;
}

// Tab icon names (Lucide)
const TAB_ICON_NAMES = {
  specification: "file-text",
  coverage: "bar-chart-3",
  sources: "folder-open",
};

// Search result item component with syntax highlighting for source
function SearchResultItem({ result, isSelected, onSelect, onHover }: SearchResultItemProps) {
  return html`
    <div
      class="search-modal-result ${isSelected ? "selected" : ""}"
      onClick=${onSelect}
      onMouseEnter=${onHover}
    >
      <div class="search-modal-result-header">
        ${result.kind === "source"
          ? html`
              <${FilePath}
                file=${result.id}
                line=${result.line > 0 ? result.line : null}
                type="source"
              />
            `
          : html`
              <${LucideIcon} name="file-text" className="search-result-icon rule" />
              <span class="search-modal-result-id">${result.id}</span>
            `}
      </div>
      ${result.kind === "source"
        ? html`
            <pre class="search-modal-result-code"><code dangerouslySetInnerHTML=${{
              __html: result.highlighted || result.content.trim(),
            }} /></pre>
          `
        : html`
            <div
              class="search-modal-result-content"
              dangerouslySetInnerHTML=${{ __html: result.highlighted || result.content.trim() }}
            />
          `}
    </div>
  `;
}

function SearchModal({ onClose, onSelect }: SearchModalProps) {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState(null);
  const [isSearching, setIsSearching] = useState(false);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const inputRef = useRef(null);
  const resultsRef = useRef(null);
  const searchTimeoutRef = useRef(null);

  // Focus input on mount and initialize Lucide icons
  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  // Re-render Lucide icons when results change
  useEffect(() => {
    if (results?.results?.length && typeof lucide !== "undefined") {
      requestAnimationFrame(() => {
        lucide.createIcons();
      });
    }
  }, [results]);

  // Debounced search
  useEffect(() => {
    if (!query || query.length < 2) {
      setResults(null);
      setSelectedIndex(0);
      return;
    }

    setIsSearching(true);

    if (searchTimeoutRef.current) {
      clearTimeout(searchTimeoutRef.current);
    }

    searchTimeoutRef.current = setTimeout(async () => {
      try {
        const res = await fetch(`/api/search?q=${encodeURIComponent(query)}&limit=50`);
        const data = await res.json();
        setResults(data);
        setSelectedIndex(0);
      } catch (e) {
        console.error("Search failed:", e);
        setResults({ results: [] });
      } finally {
        setIsSearching(false);
      }
    }, 150);

    return () => {
      if (searchTimeoutRef.current) {
        clearTimeout(searchTimeoutRef.current);
      }
    };
  }, [query]);

  // Scroll selected item into view
  useEffect(() => {
    if (!resultsRef.current) return;
    const selected = resultsRef.current.querySelector(".search-modal-result.selected");
    if (selected) {
      selected.scrollIntoView({ block: "nearest" });
    }
  }, [selectedIndex]);

  // Keyboard navigation
  const handleKeyDown = useCallback(
    (e) => {
      if (!results?.results?.length) return;

      if (e.key === "ArrowDown") {
        e.preventDefault();
        setSelectedIndex((i) => Math.min(i + 1, results.results.length - 1));
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        setSelectedIndex((i) => Math.max(i - 1, 0));
      } else if (e.key === "Enter") {
        e.preventDefault();
        const result = results.results[selectedIndex];
        if (result) onSelect(result);
      }
    },
    [results, selectedIndex, onSelect],
  );

  // Close on backdrop click
  const handleBackdropClick = useCallback(
    (e) => {
      if (e.target === e.currentTarget) {
        onClose();
      }
    },
    [onClose],
  );

  return html`
    <div class="search-overlay" onClick=${handleBackdropClick}>
      <div class="search-modal">
        <div class="search-modal-input">
          <input
            ref=${inputRef}
            type="text"
            placeholder="Search code and rules..."
            value=${query}
            onInput=${(e) => setQuery(e.target.value)}
            onKeyDown=${handleKeyDown}
          />
        </div>
        <div class="search-modal-results" ref=${resultsRef}>
          ${isSearching
            ? html` <div class="search-modal-empty">Searching...</div> `
            : results?.results?.length > 0
              ? html`
                  ${results.results.map(
                    (result, idx) => html`
                      <${SearchResultItem}
                        key=${result.kind + ":" + result.id + ":" + result.line}
                        result=${result}
                        isSelected=${idx === selectedIndex}
                        onSelect=${() => onSelect(result)}
                        onHover=${() => setSelectedIndex(idx)}
                      />
                    `,
                  )}
                `
              : query.length >= 2
                ? html` <div class="search-modal-empty">No results found</div> `
                : html` <div class="search-modal-empty">Type to search code and rules...</div> `}
        </div>
        <div class="search-modal-hint">
          <span><kbd>↑</kbd><kbd>↓</kbd> Navigate</span>
          <span><kbd>Enter</kbd> Select</span>
          <span><kbd>Esc</kbd> Close</span>
        </div>
      </div>
    </div>
  `;
}

function Header({
  view,
  onViewChange,
  onOpenSearch,
}: Omit<HeaderProps, "search" | "onSearchChange">) {
  const handleNavClick = (e, newView) => {
    e.preventDefault();
    onViewChange(newView);
  };

  return html`
    <header class="header">
      <div class="header-inner">
        <nav class="nav">
          <a
            href="/spec"
            class="nav-tab ${view === "spec" ? "active" : ""}"
            onClick=${(e) => handleNavClick(e, "spec")}
            ><${LucideIcon} name=${TAB_ICON_NAMES.specification} className="tab-icon" /><span
              >Specification</span
            ></a
          >
          <a
            href="/coverage"
            class="nav-tab ${view === "coverage" ? "active" : ""}"
            onClick=${(e) => handleNavClick(e, "coverage")}
            ><${LucideIcon} name=${TAB_ICON_NAMES.coverage} className="tab-icon" /><span
              >Coverage</span
            ></a
          >
          <a
            href="/sources"
            class="nav-tab ${view === "sources" ? "active" : ""}"
            onClick=${(e) => handleNavClick(e, "sources")}
            ><${LucideIcon} name=${TAB_ICON_NAMES.sources} className="tab-icon" /><span
              >Sources</span
            ></a
          >
        </nav>

        <div
          class="search-box"
          style="margin-left: auto; margin-right: 1rem; display: flex; align-items: center;"
        >
          <input
            type="text"
            class="search-input"
            placeholder="Search... (${modKey}+K)"
            onClick=${onOpenSearch}
            onFocus=${(e) => {
              e.target.blur();
              onOpenSearch();
            }}
            readonly
            style="cursor: pointer;"
          />
        </div>

        <a href="https://github.com/bearcove/tracey" class="logo" target="_blank" rel="noopener"
          >tracey</a
        >
      </div>
    </header>
  `;
}

// Helper to split file path into dir and filename
function splitPath(filePath) {
  const lastSlash = filePath.lastIndexOf("/");
  if (lastSlash === -1) return { dir: "", name: filePath };
  return {
    dir: filePath.slice(0, lastSlash + 1),
    name: filePath.slice(lastSlash + 1),
  };
}

// Universal file path display component
// Props:
//   file: file path
//   line: optional line number
//   short: if true, only show filename (not full path)
//   type: 'impl' | 'verify' | 'source' - affects icon color
//   onClick: optional click handler
//   className: optional additional class
function FilePath({
  file,
  line,
  short = false,
  type = "source",
  onClick,
  className = "",
}: FilePathProps) {
  const { dir, name } = splitPath(file);
  const iconClass =
    type === "impl" ? "file-path-icon-impl" : type === "verify" ? "file-path-icon-verify" : "";

  const content = html`
    <${LangIcon} filePath=${file} className="file-path-icon ${iconClass}" /><span
      class="file-path-text"
      >${!short && dir ? html`<span class="file-path-dir">${dir}</span>` : ""}<span
        class="file-path-name"
        >${name}</span
      >${line != null ? html`<span class="file-path-line">:${line}</span>` : ""}</span
    >
  `;

  if (onClick) {
    return html`
      <a
        class="file-path-link ${className}"
        href="#"
        onClick=${(e) => {
          e.preventDefault();
          onClick();
        }}
      >
        ${content}
      </a>
    `;
  }

  return html`<span class="file-path-display ${className}">${content}</span>`;
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

function CoverageView({
  data,
  search,
  level,
  onLevelChange,
  filter,
  onFilterChange,
  onSelectRule,
  onSelectFile,
}: CoverageViewProps) {
  const [levelOpen, setLevelOpen] = useState(false);

  // Close dropdowns when clicking outside
  useEffect(() => {
    const handleClick = (e) => {
      if (!e.target.closest("#level-dropdown")) setLevelOpen(false);
    };
    document.addEventListener("click", handleClick);
    return () => document.removeEventListener("click", handleClick);
  }, []);

  const allRules = useMemo(
    () => data.specs.flatMap((s) => s.rules.map((r) => ({ ...r, spec: s.name }))),
    [data],
  );

  // Infer level from rule text if not explicitly set
  const inferLevel = useCallback((rule) => {
    if (rule.level) return rule.level.toLowerCase();
    if (!rule.text) return null;
    const text = rule.text.toUpperCase();
    // Check for MUST NOT, SHALL NOT first (still MUST level)
    if (text.includes("MUST") || text.includes("SHALL") || text.includes("REQUIRED")) return "must";
    if (text.includes("SHOULD") || text.includes("RECOMMENDED")) return "should";
    if (text.includes("MAY") || text.includes("OPTIONAL")) return "may";
    return null;
  }, []);

  const filteredRules = useMemo(() => {
    let rules = allRules;

    // Filter by level (explicit or inferred from text)
    if (level !== "all") {
      rules = rules.filter((r) => inferLevel(r) === level);
    }

    // Filter by coverage (impl or verify)
    if (filter === "impl") {
      rules = rules.filter((r) => r.implRefs.length === 0);
    } else if (filter === "verify") {
      rules = rules.filter((r) => r.verifyRefs.length === 0);
    }

    // Filter by search
    if (search) {
      const q = search.toLowerCase();
      rules = rules.filter(
        (r) => r.id.toLowerCase().includes(q) || (r.text && r.text.toLowerCase().includes(q)),
      );
    }

    return rules;
  }, [allRules, search, level, filter, inferLevel]);

  const stats = useMemo(() => {
    // Stats are based on level-filtered rules (not coverage filter)
    let rules = allRules;
    if (level !== "all") {
      rules = rules.filter((r) => inferLevel(r) === level);
    }
    const total = rules.length;
    const impl = rules.filter((r) => r.implRefs.length > 0).length;
    const verify = rules.filter((r) => r.verifyRefs.length > 0).length;
    return {
      total,
      impl,
      verify,
      implPct: total ? (impl / total) * 100 : 0,
      verifyPct: total ? (verify / total) * 100 : 0,
    };
  }, [allRules, level, inferLevel]);

  // Markdown icon
  const mdIcon = html`<svg
    class="rule-icon"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    stroke-width="2"
  >
    <path d="M14 3v4a1 1 0 0 0 1 1h4" />
    <path d="M17 21H7a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h7l5 5v11a2 2 0 0 1-2 2z" />
    <path d="M9 15l2 2 4-4" />
  </svg>`;

  return html`
    <div class="stats-bar">
      <div class="stat">
        <span class="stat-label">Rules</span>
        <span class="stat-value">${stats.total}</span>
      </div>
      <div
        class="stat clickable"
        onClick=${() => onFilterChange(filter === "impl" ? null : "impl")}
      >
        <span class="stat-label">Impl Coverage ${filter === "impl" ? "(filtered)" : ""}</span>
        <span class="stat-value ${getStatClass(stats.implPct)}">${stats.implPct.toFixed(1)}%</span>
      </div>
      <div
        class="stat clickable"
        onClick=${() => onFilterChange(filter === "verify" ? null : "verify")}
      >
        <span class="stat-label">Test Coverage ${filter === "verify" ? "(filtered)" : ""}</span>
        <span class="stat-value ${getStatClass(stats.verifyPct)}"
          >${stats.verifyPct.toFixed(1)}%</span
        >
      </div>

      <!-- Level dropdown -->
      <div class="custom-dropdown ${levelOpen ? "open" : ""}" id="level-dropdown">
        <div
          class="dropdown-selected"
          onClick=${(e) => {
            e.stopPropagation();
            setLevelOpen(!levelOpen);
          }}
        >
          <span class="level-dot ${LEVELS[level].dotClass}"></span>
          <span>${LEVELS[level].name}</span>
          <svg
            class="chevron"
            width="12"
            height="12"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
          >
            <path d="M6 9l6 6 6-6" />
          </svg>
        </div>
        <div class="dropdown-menu">
          ${Object.entries(LEVELS).map(
            ([key, cfg]) => html`
              <div
                key=${key}
                class="dropdown-option ${level === key ? "active" : ""}"
                onClick=${() => {
                  onLevelChange(key);
                  setLevelOpen(false);
                }}
              >
                <span class="level-dot ${cfg.dotClass}"></span>
                <span>${cfg.name}</span>
              </div>
            `,
          )}
        </div>
      </div>
    </div>
    <div class="main">
      <div class="content">
        <div class="content-body">
          <table class="rules-table">
            <thead>
              <tr>
                <th style="width: 45%">Rule</th>
                <th style="width: 55%">References</th>
              </tr>
            </thead>
            <tbody>
              ${filteredRules.map(
                (rule) => html`
                  <tr
                    key=${rule.id}
                    onClick=${() => onSelectRule(rule.id)}
                    style="cursor: pointer;"
                  >
                    <td>
                      <div class="rule-id-row">
                        ${mdIcon}
                        <span class="rule-id">${rule.id}</span>
                      </div>
                      ${rule.text &&
                      html`<div
                        class="rule-text"
                        dangerouslySetInnerHTML=${{ __html: renderRuleText(rule.text) }}
                      />`}
                    </td>
                    <td class="rule-refs" onClick=${(e) => e.stopPropagation()}>
                      ${rule.implRefs.length > 0 || rule.verifyRefs.length > 0
                        ? html`
                            ${rule.implRefs.map(
                              (r) => html`
                                <${FileRef}
                                  key=${`impl:${r.file}:${r.line}`}
                                  file=${r.file}
                                  line=${r.line}
                                  type="impl"
                                  onSelectFile=${onSelectFile}
                                />
                              `,
                            )}
                            ${rule.verifyRefs.map(
                              (r) => html`
                                <${FileRef}
                                  key=${`verify:${r.file}:${r.line}`}
                                  file=${r.file}
                                  line=${r.line}
                                  type="verify"
                                  onSelectFile=${onSelectFile}
                                />
                              `,
                            )}
                          `
                        : html`<span style="color: var(--fg-dim)">—</span>`}
                    </td>
                  </tr>
                `,
              )}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  `;
}

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

function FileTree({
  node,
  selectedFile,
  onSelectFile,
  depth = 0,
  search,
  parentPath = "",
}: FileTreeProps) {
  // Check if selected file is in this subtree
  const currentPath = parentPath ? `${parentPath}/${node.name}` : node.name;
  const containsSelectedFile = selectedFile?.startsWith(currentPath + "/");
  const hasSelectedFile =
    selectedFile && (containsSelectedFile || node.files.some((f) => f.path === selectedFile));

  const [open, setOpen] = useState(depth < 2 || hasSelectedFile);

  // Auto-expand when selected file changes to be in this subtree
  useEffect(() => {
    if (hasSelectedFile && !open) {
      setOpen(true);
    }
  }, [selectedFile, hasSelectedFile]);

  const folders = Object.values(node.children).sort((a, b) => a.name.localeCompare(b.name));
  const files = node.files.sort((a, b) => a.name.localeCompare(b.name));

  // Filter if searching
  const matchesSearch = (path) => {
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
            ${hasChildren
              ? html`
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
                `
              : html`<span class="outline-toggle-spacer"></span>`}
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
    const item = e.target.closest(".refs-popup-item");
    if (item) {
      const file = item.dataset.file;
      const line = parseInt(item.dataset.line, 10);
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

function SpecView({
  config,
  version,
  selectedRule,
  selectedHeading,
  onSelectRule,
  onSelectFile,
  scrollPosition,
  onScrollChange,
}: SpecViewProps) {
  const spec = useSpec(config.specs[0]?.name, version);
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
          ${spec.name}${spec.sections.length > 1 ? ` (${spec.sections.length} files)` : ""}
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
