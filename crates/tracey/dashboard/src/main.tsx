import htm from "htm";
import { h, render } from "preact";
import { useCallback, useEffect, useMemo, useRef, useState } from "preact/hooks";
import { LocationProvider, Router, Route, useLocation, useRoute } from "preact-iso";
import "./style.css";

// Types
import type {
  Editor,
  FileContent,
  FilePathProps,
  FileRefProps,
  HeaderProps,
  OutlineEntry,
  SpecViewProps,
  SourcesViewProps,
  CoverageViewProps,
  ViewType,
} from "./types";

// Modules
import { useApi, useFile, useSpec } from "./hooks";
import { buildUrl } from "./router";
import { EDITORS, LEVELS, LANG_DEVICON_MAP, TAB_ICON_NAMES, modKey } from "./config";
import {
  buildFileTree,
  getStatClass,
  getCoverageBadge,
  renderRuleText,
  splitHighlightedHtml,
  splitPath,
} from "./utils";

// Views (to be imported once moved)
import { SpecView } from "./views/spec";
import { SourcesView } from "./views/sources";
import { CoverageView } from "./views/coverage";

const html = htm.bind(h);

// Declare lucide as global (loaded via CDN)
declare const lucide: { createIcons: (opts?: { nodes?: NodeList }) => void };

// ========================================================================
// Components
// ========================================================================

function Header({ view, onViewChange, onOpenSearch }: HeaderProps) {
  const tabs = [
    { id: "spec", label: "Specification", icon: TAB_ICON_NAMES.specification },
    { id: "coverage", label: "Coverage", icon: TAB_ICON_NAMES.coverage },
    { id: "sources", label: "Sources", icon: TAB_ICON_NAMES.sources },
  ];

  return html`
    <header class="header">
      <div class="tabs">
        ${tabs.map(
          (tab) => html`
            <button
              key=${tab.id}
              class="tab ${view === tab.id ? "active" : ""}"
              onClick=${() => onViewChange(tab.id as ViewType)}
            >
              <i data-lucide=${tab.icon}></i>
              <span>${tab.label}</span>
            </button>
          `,
        )}
      </div>
      <div class="header-actions">
        <button class="search-button" onClick=${onOpenSearch}>
          <i data-lucide="search"></i>
          <span>Search</span>
          <kbd>${modKey}+K</kbd>
        </button>
      </div>
    </header>
  `;
}

// SVG arc indicator for coverage progress
interface CoverageArcProps {
  count: number;
  total: number;
  color: string;
  title?: string;
  size?: number;
}

function CoverageArc({ count, total, color, title, size = 20 }: CoverageArcProps) {
  const pct = total > 0 ? count / total : 0;
  const radius = (size - 4) / 2;
  const circumference = 2 * Math.PI * radius;
  const strokeDasharray = `${pct * circumference} ${circumference}`;
  const center = size / 2;

  return html`
    <svg
      class="coverage-arc"
      width=${size}
      height=${size}
      viewBox="0 0 ${size} ${size}"
      title=${title}
    >
      <circle
        cx=${center}
        cy=${center}
        r=${radius}
        fill="none"
        stroke="var(--border)"
        stroke-width="1.5"
      />
      <circle
        cx=${center}
        cy=${center}
        r=${radius}
        fill="none"
        stroke=${color}
        stroke-width="3"
        stroke-dasharray=${strokeDasharray}
        stroke-linecap="round"
        transform="rotate(-90 ${center} ${center})"
      />
      <text
        x=${center}
        y=${center}
        text-anchor="middle"
        dominant-baseline="central"
        font-size="7"
        fill="var(--fg-muted)"
      >
        ${count}
      </text>
    </svg>
  `;
}

// File path display component
function FilePath({ file, line, short, type, onClick, className = "" }: FilePathProps) {
  const { dir, name } = splitPath(file);
  const lineStr = line ? `:${line}` : "";

  const typeClass = type === "impl" ? "impl" : type === "verify" ? "verify" : "";
  const typeLabel = type === "impl" ? "impl" : type === "verify" ? "test" : "";

  return html`
    <span class="file-path ${className} ${onClick ? "clickable" : ""}" onClick=${onClick}>
      ${type && html`<span class="file-type-badge ${typeClass}">${typeLabel}</span>`}
      ${!short && dir && html`<span class="file-dir">${dir}</span>`}
      <span class="file-name">${name}${lineStr}</span>
    </span>
  `;
}

// File reference component
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

// Show a popup with all references
function showRefsPopup(
  e: Event,
  refs: Array<{ file: string; line: number }>,
  badgeElement: HTMLElement,
  onSelectFile: (file: string, line: number) => void,
) {
  const existing = document.querySelector(".refs-popup");
  if (existing) existing.remove();

  const popup = document.createElement("div");
  popup.className = "refs-popup";

  const rect = badgeElement.getBoundingClientRect();
  popup.style.position = "fixed";
  popup.style.top = `${rect.bottom + 8}px`;
  popup.style.left = `${rect.left}px`;
  popup.style.zIndex = "10000";

  const items = refs
    .map((ref) => {
      const filename = ref.file.split("/").pop();
      return `<div class="refs-popup-item" data-file="${ref.file}" data-line="${ref.line}">
        <span class="refs-popup-file">${filename}:${ref.line}</span>
      </div>`;
    })
    .join("");

  popup.innerHTML = `<div class="refs-popup-inner">${items}</div>`;

  popup.addEventListener("click", (e) => {
    const item = (e.target as HTMLElement).closest(".refs-popup-item") as HTMLElement | null;
    if (item) {
      const file = item.dataset.file;
      const line = parseInt(item.dataset.line || "0", 10);
      if (file) onSelectFile(file, line);
      popup.remove();
    }
  });

  const closeHandler = (e: Event) => {
    if (!popup.contains(e.target as Node) && !badgeElement.contains(e.target as Node)) {
      popup.remove();
      document.removeEventListener("click", closeHandler);
    }
  };
  setTimeout(() => document.addEventListener("click", closeHandler), 0);

  document.body.appendChild(popup);
}

// ========================================================================
// App with preact-iso Router
// ========================================================================

function App() {
  const { data, error, version } = useApi();
  const { route } = useLocation();
  const [searchOpen, setSearchOpen] = useState(false);

  // Initialize Lucide icons
  useEffect(() => {
    if (typeof lucide !== "undefined") {
      lucide.createIcons();
    }
  }, []);

  if (error) return html`<div class="loading">Error: ${error}</div>`;
  if (!data) return html`<div class="loading">Loading...</div>`;

  const { config, forward, reverse } = data;

  // Get current spec from URL or default to first
  const defaultSpec = config.specs?.[0]?.name || null;

  const handleViewChange = useCallback(
    (newView: string) => {
      // Get current spec from URL
      const currentSpec = window.location.pathname.split("/")[1] || defaultSpec;
      route(buildUrl(currentSpec, newView as any));
    },
    [route, defaultSpec],
  );

  const handleOpenSearch = useCallback(() => {
    setSearchOpen(true);
  }, []);

  // Global keyboard shortcut for search
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
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

  // Determine current view from pathname
  const pathParts = window.location.pathname.split("/").filter(Boolean);
  const currentView = pathParts[1] || "spec";

  return html`
    <div class="layout">
      <${Header}
        view=${currentView}
        onViewChange=${handleViewChange}
        onOpenSearch=${handleOpenSearch}
      />
      <${Router}>
        <${Route}
          path="/"
          component=${() => {
            // Redirect to default spec
            useEffect(() => {
              if (defaultSpec) route(`/${defaultSpec}/spec`, true);
            }, []);
            return html`<div class="loading">Redirecting...</div>`;
          }}
        />
        <${Route} path="/:spec/spec/:heading*" component=${SpecViewRoute} />
        <${Route} path="/:spec/sources/:file*" component=${SourcesViewRoute} />
        <${Route} path="/:spec/coverage" component=${CoverageViewRoute} />
        <${Route}
          path="/:spec"
          component=${() => {
            const { params } = useRoute();
            useEffect(() => {
              route(`/${params.spec}/spec`, true);
            }, [params.spec]);
            return html`<div class="loading">Redirecting...</div>`;
          }}
        />
        <${Route} default component=${() => html`<div class="empty-state">Page not found</div>`} />
      <//>
    </div>
  `;
}

// Route components that extract params and render views
function SpecViewRoute() {
  const { params, query } = useRoute();
  const { route } = useLocation();
  const { data, version } = useApi();

  if (!data) return html`<div class="loading">Loading...</div>`;

  const { config, forward } = data;
  const spec = params.spec;
  const heading = params.heading || null;
  const rule = query.rule || null;

  const [scrollPosition, setScrollPosition] = useState(0);

  const handleSelectSpec = useCallback(
    (specName: string) => {
      route(buildUrl(specName, "spec", { heading }));
    },
    [route, heading],
  );

  const handleSelectRule = useCallback(
    (ruleId: string) => {
      route(buildUrl(spec, "spec", { rule: ruleId }));
    },
    [route, spec],
  );

  const handleSelectFile = useCallback(
    (file: string, line?: number | null, context?: string | null) => {
      route(buildUrl(spec, "sources", { file, line, context }));
    },
    [route, spec],
  );

  return html`
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
      scrollPosition=${scrollPosition}
      onScrollChange=${setScrollPosition}
    />
  `;
}

function SourcesViewRoute() {
  const { params, query } = useRoute();
  const { route } = useLocation();
  const { data } = useApi();

  if (!data) return html`<div class="loading">Loading...</div>`;

  const { config, forward, reverse } = data;
  const spec = params.spec;

  // Parse file:line from the file param
  let file: string | null = params.file || null;
  let line: number | null = null;
  if (file) {
    const colonIdx = file.lastIndexOf(":");
    if (colonIdx !== -1) {
      const possibleLine = parseInt(file.slice(colonIdx + 1), 10);
      if (!Number.isNaN(possibleLine)) {
        line = possibleLine;
        file = file.slice(0, colonIdx);
      }
    }
  }
  const context = query.context || null;

  const [search, setSearch] = useState("");

  const handleSelectFile = useCallback(
    (filePath: string, lineNum?: number | null, ruleContext?: string | null) => {
      route(buildUrl(spec, "sources", { file: filePath, line: lineNum, context: ruleContext }));
    },
    [route, spec],
  );

  const handleSelectRule = useCallback(
    (ruleId: string) => {
      route(buildUrl(spec, "spec", { rule: ruleId }));
    },
    [route, spec],
  );

  const handleClearContext = useCallback(() => {
    route(buildUrl(spec, "sources", { file, line, context: null }), true);
  }, [route, spec, file, line]);

  return html`
    <${SourcesView}
      data=${reverse}
      forward=${forward}
      config=${config}
      search=${search}
      selectedFile=${file}
      selectedLine=${line}
      ruleContext=${context}
      onSelectFile=${handleSelectFile}
      onSelectRule=${handleSelectRule}
      onClearContext=${handleClearContext}
    />
  `;
}

function CoverageViewRoute() {
  const { params, query } = useRoute();
  const { route } = useLocation();
  const { data } = useApi();

  if (!data) return html`<div class="loading">Loading...</div>`;

  const { config, forward } = data;
  const spec = params.spec;
  const filter = query.filter || null;
  const level = query.level || "all";

  const [search, setSearch] = useState("");

  const handleLevelChange = useCallback(
    (newLevel: string) => {
      route(buildUrl(spec, "coverage", { filter, level: newLevel }));
    },
    [route, spec, filter],
  );

  const handleFilterChange = useCallback(
    (newFilter: string | null) => {
      route(buildUrl(spec, "coverage", { filter: newFilter, level }));
    },
    [route, spec, level],
  );

  const handleSelectRule = useCallback(
    (ruleId: string) => {
      route(buildUrl(spec, "spec", { rule: ruleId }));
    },
    [route, spec],
  );

  const handleSelectFile = useCallback(
    (file: string, lineNum?: number | null, context?: string | null) => {
      route(buildUrl(spec, "sources", { file, line: lineNum, context }));
    },
    [route, spec],
  );

  return html`
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
  `;
}

// ========================================================================
// Mount
// ========================================================================

render(
  html`
    <${LocationProvider}>
      <${App} />
    <//>
  `,
  document.getElementById("app")!,
);

// Global keyboard shortcuts
document.addEventListener("keydown", (e) => {
  if ((e.metaKey || e.ctrlKey) && e.key === "k") {
    e.preventDefault();
    (document.querySelector(".search-input") as HTMLElement | null)?.focus();
  }
});

// Export shared components for views
export { html, CoverageArc, FilePath, FileRef, showRefsPopup };
