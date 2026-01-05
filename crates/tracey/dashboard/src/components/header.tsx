// Header component - view tabs and search

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
