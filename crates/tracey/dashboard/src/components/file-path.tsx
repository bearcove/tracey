// File path display component

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
