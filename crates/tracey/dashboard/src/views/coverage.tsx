// Coverage view - rule coverage table
import { h } from "preact";
import { useCallback, useEffect, useMemo, useState } from "preact/hooks";
import type { CoverageViewProps } from "../types";
import { LEVELS } from "../config";
import { getStatClass, renderRuleText } from "../utils";
import { html, FilePath, FileRef } from "../main";

export function CoverageView({
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
    const handleClick = (e: Event) => {
      if (!(e.target as HTMLElement).closest("#level-dropdown")) setLevelOpen(false);
    };
    document.addEventListener("click", handleClick);
    return () => document.removeEventListener("click", handleClick);
  }, []);

  const allRules = useMemo(
    () => data.specs.flatMap((s) => s.rules.map((r) => ({ ...r, spec: s.name }))),
    [data],
  );

  // Infer level from rule text if not explicitly set
  const inferLevel = useCallback((rule: { level?: string; text?: string }) => {
    if (rule.level) return rule.level.toLowerCase();
    if (!rule.text) return null;
    const text = rule.text.toUpperCase();
    if (text.includes("MUST") || text.includes("SHALL") || text.includes("REQUIRED")) return "must";
    if (text.includes("SHOULD") || text.includes("RECOMMENDED")) return "should";
    if (text.includes("MAY") || text.includes("OPTIONAL")) return "may";
    return null;
  }, []);

  const filteredRules = useMemo(() => {
    let rules = allRules;

    // Filter by level
    if (level !== "all") {
      rules = rules.filter((r) => inferLevel(r) === level);
    }

    // Filter by coverage
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

      <div class="custom-dropdown ${levelOpen ? "open" : ""}" id="level-dropdown">
        <div
          class="dropdown-selected"
          onClick=${(e: Event) => {
            e.stopPropagation();
            setLevelOpen(!levelOpen);
          }}
        >
          <span class="level-dot ${LEVELS[level]?.dotClass || ""}"></span>
          <span>${LEVELS[level]?.name || "All"}</span>
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
                    <td class="rule-refs" onClick=${(e: Event) => e.stopPropagation()}>
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
