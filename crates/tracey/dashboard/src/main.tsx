import htm from "htm";
import { createContext, h, render } from "preact";
import { useCallback, useContext, useEffect, useRef, useState } from "preact/hooks";
import { LocationProvider, Route, Router, useLocation, useRoute } from "preact-iso";
import "./style.css";

import { getDeviconClass, modKey, TAB_ICON_NAMES } from "./config";

// Modules
import { useApi, UseApiResult } from "./hooks";
import { buildUrl } from "./router";
// Types
import type {
  FilePathProps,
  FileRefProps,
  HeaderProps,
  LangIconProps,
  LucideIconProps,
  SearchModalProps,
  SearchResult,
  SearchResultItemProps,
  ViewType,
} from "./types";
import { splitPath } from "./utils";
import { CoverageView } from "./views/coverage";
import { SourcesView } from "./views/sources";
// Views (to be imported once moved)
import { SpecView } from "./views/spec";

const html = htm.bind(h);

// Declare lucide as global (loaded via CDN)
declare const lucide: { createIcons: (opts?: { nodes?: NodeList }) => void };

// Context to share API data across route components
const ApiContext = createContext<UseApiResult | null>(null);

function useApiContext(): UseApiResult {
  const ctx = useContext(ApiContext);
  if (!ctx) throw new Error("useApiContext must be used within ApiContext.Provider");
  return ctx;
}

// ========================================================================
// Components
// ========================================================================

function LucideIcon({ name, className = "" }: LucideIconProps) {
  const iconRef = useRef<HTMLSpanElement>(null);

  useEffect(() => {
    if (iconRef.current && typeof lucide !== "undefined") {
      iconRef.current.innerHTML = "";
      const i = document.createElement("i");
      i.setAttribute("data-lucide", name);
      iconRef.current.appendChild(i);
      lucide.createIcons({ nodes: [i] as unknown as NodeList });
    }
  }, [name]);

  return html`<span ref=${iconRef} class=${className}></span>`;
}

// Language icon component - uses devicon if available, falls back to Lucide
function LangIcon({ filePath, className = "" }: LangIconProps) {
  const deviconClass = getDeviconClass(filePath);
  const iconRef = useRef<HTMLSpanElement>(null);

  // For Lucide fallback
  useEffect(() => {
    if (!deviconClass && iconRef.current && typeof lucide !== "undefined") {
      iconRef.current.innerHTML = "";
      const i = document.createElement("i");
      i.setAttribute("data-lucide", "file");
      iconRef.current.appendChild(i);
      lucide.createIcons({ nodes: [i] as unknown as NodeList });
    }
  }, [deviconClass]);

  if (deviconClass) {
    return html`<i class="${deviconClass} ${className}"></i>`;
  }
  return html`<span ref=${iconRef} class=${className}></span>`;
}

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

// [impl dashboard.search.modal]
// [impl dashboard.search.rules]
// [impl dashboard.search.files]
// [impl dashboard.search.navigation]
function SearchModal({ onClose, onSelect }: SearchModalProps) {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<{ results: SearchResult[] } | null>(null);
  const [isSearching, setIsSearching] = useState(false);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const resultsRef = useRef<HTMLDivElement>(null);
  const searchTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Focus input on mount
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
    (e: KeyboardEvent) => {
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
    (e: MouseEvent) => {
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
            onInput=${(e: Event) => setQuery((e.target as HTMLInputElement).value)}
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

// [impl dashboard.header.nav-tabs]
// [impl dashboard.header.nav-active]
// [impl dashboard.header.nav-preserve-spec]
// [impl dashboard.header.search]
// [impl dashboard.header.logo]
function Header({
  view,
  spec,
  impl,
  config,
  onViewChange,
  onSpecChange,
  onImplChange,
  onOpenSearch,
}: HeaderProps) {
  const handleNavClick = (e: Event, newView: ViewType) => {
    e.preventDefault();
    onViewChange(newView);
  };

  const specBase = spec && impl ? `/${encodeURIComponent(spec)}/${encodeURIComponent(impl)}` : "";

  // Get available implementations for current spec
  const currentSpecInfo = config.specs?.find((s) => s.name === spec);
  const implementations = currentSpecInfo?.implementations || [];

  // [impl dashboard.spec.switcher]
  // [impl dashboard.spec.switcher-single]
  // Always show spec and impl dropdowns, even with single options
  return html`
    <header class="header">
      <div class="header-inner">
        <div class="header-pickers">
          <select
            class="header-select spec-select"
            value=${spec || ""}
            onChange=${(e: Event) => onSpecChange((e.target as HTMLSelectElement).value)}
          >
            ${config.specs?.map(
              (s) => html` <option key=${s.name} value=${s.name}>${s.name}</option> `,
            )}
          </select>
          <select
            class="header-select impl-select"
            value=${impl || ""}
            onChange=${(e: Event) => onImplChange((e.target as HTMLSelectElement).value)}
          >
            ${implementations.map((i) => html` <option key=${i} value=${i}>${i}</option> `)}
          </select>
        </div>

        ${/* r[impl dashboard.header.nav-tabs] */ null}
        ${/* r[impl dashboard.header.nav-active] */ null}
        ${/* r[impl dashboard.header.nav-preserve-spec] */ null}
        <nav class="nav">
          <a
            href="${specBase}/spec"
            class="nav-tab ${view === "spec" ? "active" : ""}"
            onClick=${(e: Event) => handleNavClick(e, "spec")}
            ><${LucideIcon} name=${TAB_ICON_NAMES.specification} className="tab-icon" /><span
              >Specification</span
            ></a
          >
          <a
            href="${specBase}/coverage"
            class="nav-tab ${view === "coverage" ? "active" : ""}"
            onClick=${(e: Event) => handleNavClick(e, "coverage")}
            ><${LucideIcon} name=${TAB_ICON_NAMES.coverage} className="tab-icon" /><span
              >Coverage</span
            ></a
          >
          <a
            href="${specBase}/sources"
            class="nav-tab ${view === "sources" ? "active" : ""}"
            onClick=${(e: Event) => handleNavClick(e, "sources")}
            ><${LucideIcon} name=${TAB_ICON_NAMES.sources} className="tab-icon" /><span
              >Sources</span
            ></a
          >
        </nav>

        <div
          class="search-box"
          style="margin-left: auto; margin-right: 1rem; display: flex; align-items: center;"
        >
          ${/* r[impl dashboard.header.search] */ null}
          <input
            type="text"
            class="search-input"
            placeholder="Search... (${modKey}+K)"
            onClick=${onOpenSearch}
            onFocus=${(e: FocusEvent) => {
              (e.target as HTMLInputElement).blur();
              onOpenSearch();
            }}
            readonly
            style="cursor: pointer;"
          />
        </div>

        ${/* r[impl dashboard.header.logo] */ null}
        <a href="https://github.com/bearcove/tracey" class="logo" target="_blank" rel="noopener"
          >tracey</a
        >
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
  hideNumber?: boolean;
}

function CoverageArc({
  count,
  total,
  color,
  title,
  size = 20,
  hideNumber = false,
}: CoverageArcProps) {
  const pct = total > 0 ? count / total : 0;
  const isComplete = total > 0 && count === total;
  const radius = (size - 4) / 2;
  const circumference = 2 * Math.PI * radius;
  const strokeDasharray = `${pct * circumference} ${circumference}`;
  const center = size / 2;

  // Show checkmark when 100% coverage
  if (isComplete) {
    return html`
      <svg
        class="coverage-arc coverage-arc--complete"
        width=${size}
        height=${size}
        viewBox="0 0 ${size} ${size}"
        title=${title}
      >
        <circle cx=${center} cy=${center} r=${radius} fill=${color} opacity="0.15" />
        <path
          d="M${center - 4} ${center} l2.5 2.5 l5 -5"
          fill="none"
          stroke=${color}
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
        />
      </svg>
    `;
  }

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
      ${!hideNumber &&
      html`
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
      `}
    </svg>
  `;
}

// File path display component
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
        onClick=${(e: Event) => {
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
  _e: Event,
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
  const apiResult = useApi();
  const { data, error } = apiResult;
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

  // Get defaults from config
  const defaultSpec = config.specs?.[0]?.name || null;
  const defaultImpl = config.specs?.[0]?.implementations?.[0] || null;

  // Determine current spec, impl, and view from pathname
  // URL format: /:spec/:impl/:view/...
  const pathParts = window.location.pathname.split("/").filter(Boolean);
  const currentSpec = pathParts[0] || defaultSpec;
  const currentImpl = pathParts[1] || defaultImpl;
  const currentView = pathParts[2] || "spec";

  const handleViewChange = useCallback(
    (newView: string) => {
      route(buildUrl(currentSpec, currentImpl, newView as any));
    },
    [route, currentSpec, currentImpl],
  );

  const handleSpecChange = useCallback(
    (newSpec: string) => {
      // When changing spec, reset to first implementation of new spec
      const newSpecInfo = config.specs?.find((s) => s.name === newSpec);
      const newImpl = newSpecInfo?.implementations?.[0] || currentImpl;
      route(buildUrl(newSpec, newImpl, currentView as any));
    },
    [route, config, currentView, currentImpl],
  );

  const handleImplChange = useCallback(
    (newImpl: string) => {
      route(buildUrl(currentSpec, newImpl, currentView as any));
    },
    [route, currentSpec, currentView],
  );

  const handleOpenSearch = useCallback(() => {
    setSearchOpen(true);
  }, []);

  const handleSearchSelect = useCallback(
    (result: SearchResult) => {
      setSearchOpen(false);
      if (result.kind === "rule") {
        route(buildUrl(currentSpec, currentImpl, "spec", { rule: result.id }));
      } else {
        route(
          buildUrl(currentSpec, currentImpl, "sources", { file: result.id, line: result.line }),
        );
      }
    },
    [route, currentSpec, currentImpl],
  );

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

  return html`
    <${ApiContext.Provider} value=${apiResult}>
      <div class="layout">
        <${Header}
          view=${currentView}
          spec=${currentSpec}
          impl=${currentImpl}
          config=${config}
          onViewChange=${handleViewChange}
          onSpecChange=${handleSpecChange}
          onImplChange=${handleImplChange}
          onOpenSearch=${handleOpenSearch}
        />
        ${searchOpen &&
        html`
          <${SearchModal} onClose=${() => setSearchOpen(false)} onSelect=${handleSearchSelect} />
        `}
        <${Router}>
          <${Route}
            path="/"
            component=${() => {
              // [impl dashboard.url.root-redirect]
              // Redirect to default spec/impl
              useEffect(() => {
                if (defaultSpec && defaultImpl) route(`/${defaultSpec}/${defaultImpl}/spec`, true);
              }, []);
              return html`<div class="loading">Redirecting...</div>`;
            }}
          />
          <${Route} path="/:spec/:impl/spec" component=${SpecViewRoute} />
          <${Route} path="/:spec/:impl/sources/:file*" component=${SourcesViewRoute} />
          <${Route} path="/:spec/:impl/coverage" component=${CoverageViewRoute} />
          <${Route}
            path="/:spec/:impl"
            component=${() => {
              // r[impl dashboard.url.structure]
              const { params } = useRoute();
              useEffect(() => {
                route(`/${params.spec}/${params.impl}/spec`, true);
              }, [params.spec, params.impl]);
              return html`<div class="loading">Redirecting...</div>`;
            }}
          />
          <${Route}
            path="/:spec"
            component=${() => {
              // Legacy URL without impl - redirect with default impl
              const { params } = useRoute();
              useEffect(() => {
                const specInfo = config.specs?.find((s) => s.name === params.spec);
                const impl = specInfo?.implementations?.[0] || defaultImpl;
                route(`/${params.spec}/${impl}/spec`, true);
              }, [params.spec]);
              return html`<div class="loading">Redirecting...</div>`;
            }}
          />
          <${Route}
            default
            component=${() => {
              // [impl dashboard.url.invalid-spec]
              return html`<div class="empty-state">Page not found</div>`;
            }}
          />
        <//>
      </div>
    <//>
  `;
}

// Route components that extract params and render views
// r[impl dashboard.url.spec-view]
function SpecViewRoute() {
  const { params, query } = useRoute();
  const { route } = useLocation();
  const { data, version } = useApiContext();

  if (!data) return html`<div class="loading">Loading...</div>`;

  const { config, forward } = data;
  const spec = params.spec;
  const impl = params.impl;
  // Hash can be a heading (e.g., #my-heading) or a requirement (e.g., #r--my.rule)
  const hash = window.location.hash ? window.location.hash.slice(1) : null;
  const rule = hash?.startsWith("r--") ? hash.slice(3) : null;
  const heading = hash && !hash.startsWith("r--") ? hash : null;

  const [scrollPosition, setScrollPosition] = useState(0);

  const handleSelectSpec = useCallback(
    (specName: string) => {
      route(buildUrl(specName, impl, "spec", { heading }));
    },
    [route, impl, heading],
  );

  const handleSelectRule = useCallback(
    (ruleId: string) => {
      route(buildUrl(spec, impl, "spec", { rule: ruleId }));
    },
    [route, spec, impl],
  );

  const handleSelectFile = useCallback(
    (file: string, line?: number | null, context?: string | null) => {
      route(buildUrl(spec, impl, "sources", { file, line, context }));
    },
    [route, spec, impl],
  );

  return html`
    <${SpecView}
      config=${config}
      forward=${forward}
      version=${version}
      selectedSpec=${spec}
      selectedImpl=${impl}
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

// r[impl dashboard.url.sources-view]
// r[impl dashboard.url.context]
function SourcesViewRoute() {
  const { params, query } = useRoute();
  const { route } = useLocation();
  const { data } = useApiContext();

  if (!data) return html`<div class="loading">Loading...</div>`;

  const { config, forward, reverse } = data;
  const spec = params.spec;
  const impl = params.impl;

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

  const [search, _setSearch] = useState("");

  const handleSelectFile = useCallback(
    (filePath: string, lineNum?: number | null, ruleContext?: string | null) => {
      route(
        buildUrl(spec, impl, "sources", {
          file: filePath,
          line: lineNum,
          context: ruleContext,
        }),
      );
    },
    [route, spec, impl],
  );

  const handleSelectRule = useCallback(
    (ruleId: string) => {
      route(buildUrl(spec, impl, "spec", { rule: ruleId }));
    },
    [route, spec, impl],
  );

  const handleClearContext = useCallback(() => {
    route(buildUrl(spec, impl, "sources", { file, line, context: null }), true);
  }, [route, spec, impl, file, line]);

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

// r[impl dashboard.url.coverage-view]
function CoverageViewRoute() {
  const { params, query } = useRoute();
  const { route } = useLocation();
  const { data } = useApiContext();

  if (!data) return html`<div class="loading">Loading...</div>`;

  const { config, forward } = data;
  const spec = params.spec;
  const impl = params.impl;
  const filter = query.filter || null;
  const level = query.level || "all";

  const [search, setSearch] = useState("");

  const handleLevelChange = useCallback(
    (newLevel: string) => {
      route(buildUrl(spec, impl, "coverage", { filter, level: newLevel }));
    },
    [route, spec, impl, filter],
  );

  const handleFilterChange = useCallback(
    (newFilter: string | null) => {
      route(buildUrl(spec, impl, "coverage", { filter: newFilter, level }));
    },
    [route, spec, impl, level],
  );

  const handleSelectRule = useCallback(
    (ruleId: string) => {
      route(buildUrl(spec, impl, "spec", { rule: ruleId }));
    },
    [route, spec, impl],
  );

  const handleSelectFile = useCallback(
    (file: string, lineNum?: number | null, context?: string | null) => {
      route(buildUrl(spec, impl, "sources", { file, line: lineNum, context }));
    },
    [route, spec, impl],
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
export { html, CoverageArc, FilePath, FileRef, LangIcon, showRefsPopup };
