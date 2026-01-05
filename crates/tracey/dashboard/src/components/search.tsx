// Search modal component

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
