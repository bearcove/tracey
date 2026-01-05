// File tree component for sources view sidebar

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
