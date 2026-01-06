import { useEffect, useRef, useState } from "preact/hooks";
import { html } from "../main";
import { EditorView, keymap } from "@codemirror/view";
import { EditorState } from "@codemirror/state";
import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
import { markdown } from "@codemirror/lang-markdown";
import { vim } from "@replit/codemirror-vim";

interface MarkdownEditorProps {
  filePath: string;
  byteRange: string; // "start-end"
  onClose: () => void;
}

export function MarkdownEditor({ filePath, byteRange, onClose }: MarkdownEditorProps) {
  const [content, setContent] = useState("");
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const editorRef = useRef<HTMLDivElement>(null);
  const editorViewRef = useRef<EditorView | null>(null);

  const [start, end] = byteRange.split("-").map(Number);

  // Fetch content on mount
  useEffect(() => {
    const fetchContent = async () => {
      try {
        const params = new URLSearchParams({
          path: filePath,
          start: start.toString(),
          end: end.toString(),
        });
        const response = await fetch(`/api/file-range?${params}`);
        if (!response.ok) {
          throw new Error("Failed to fetch content");
        }
        const data = await response.json();
        setContent(data.content);
        setLoading(false);
      } catch (err) {
        setError(err instanceof Error ? err.message : "Failed to load");
        setLoading(false);
      }
    };
    fetchContent();
  }, [filePath, start, end]);

  // Initialize CodeMirror when content loads
  useEffect(() => {
    if (!loading && !error && editorRef.current && !editorViewRef.current) {
      const startState = EditorState.create({
        doc: content,
        extensions: [
          vim(),
          markdown(),
          history(),
          keymap.of([...defaultKeymap, ...historyKeymap]),
          EditorView.lineWrapping,
          EditorView.theme({
            "&": {
              height: "100%",
              fontSize: "0.9rem",
              fontFamily: "var(--font-mono)",
            },
            ".cm-scroller": {
              fontFamily: "var(--font-mono)",
              overflow: "auto",
            },
            ".cm-content": {
              padding: "1rem",
              fontVariationSettings: '"MONO" 1, "CASL" 0',
            },
            ".cm-gutters": {
              backgroundColor: "var(--bg-secondary)",
              borderRight: "1px solid var(--border)",
              color: "var(--fg-dim)",
            },
            ".cm-activeLineGutter": {
              backgroundColor: "var(--hover)",
            },
            ".cm-line": {
              padding: "0 0.5rem",
            },
            "&.cm-focused": {
              outline: "none",
            },
            "&.cm-focused .cm-cursor": {
              borderLeftColor: "var(--accent)",
            },
            ".cm-selectionBackground": {
              backgroundColor: "var(--accent-dim) !important",
            },
            "&.cm-focused .cm-selectionBackground": {
              backgroundColor: "var(--accent-dim) !important",
            },
          }),
        ],
      });

      const view = new EditorView({
        state: startState,
        parent: editorRef.current,
      });

      editorViewRef.current = view;

      // Focus editor
      view.focus();

      return () => {
        view.destroy();
        editorViewRef.current = null;
      };
    }
  }, [loading, error, content]);

  // Handle Escape key
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        onClose();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [onClose]);

  const handleSave = async () => {
    if (!editorViewRef.current) return;

    const newContent = editorViewRef.current.state.doc.toString();
    setSaving(true);
    setError(null);
    try {
      const response = await fetch("/api/file-range", {
        method: "PATCH",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          path: filePath,
          start,
          end,
          content: newContent,
        }),
      });
      if (!response.ok) {
        throw new Error("Failed to save");
      }
      // Success! Close modal and let file watcher trigger rebuild
      onClose();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to save");
      setSaving(false);
    }
  };

  return html`
    <div class="modal-overlay" onClick=${onClose}>
      <div class="modal-content editor-modal" onClick=${(e: Event) => e.stopPropagation()}>
        <div class="modal-header">
          <h3>Edit Requirement</h3>
          <div class="modal-vim-indicator">VIM MODE</div>
          <button class="modal-close" onClick=${onClose} title="Close (Esc)">×</button>
        </div>
        <div class="modal-body">
          ${
            loading
              ? html`<div class="modal-loading">Loading...</div>`
              : error
                ? html`<div class="modal-error">${error}</div>`
                : html`<div class="markdown-editor-wrapper" ref=${editorRef} />`
          }
        </div>
        <div class="modal-footer">
          <div class="modal-info">
            <code>${filePath}</code>
            <span class="modal-range">${start}-${end}</span>
          </div>
          <div class="modal-actions">
            <button class="modal-btn modal-btn-cancel" onClick=${onClose} disabled=${saving}>
              Cancel
            </button>
            <button
              class="modal-btn modal-btn-save"
              onClick=${handleSave}
              disabled=${loading || saving}
            >
              ${saving ? "Saving..." : "Save"}
            </button>
          </div>
        </div>
      </div>
    </div>
  `;
}
