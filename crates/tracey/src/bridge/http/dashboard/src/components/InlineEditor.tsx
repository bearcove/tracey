// r[impl dashboard.editing.inline.fullwidth]
// r[impl dashboard.editing.inline.vim-mode]
// r[impl dashboard.editing.inline.codemirror]
// r[impl dashboard.editing.inline.header]
// r[impl dashboard.editing.inline.dimensions]
// r[impl dashboard.editing.save.patch-file]
// r[impl dashboard.editing.save.error-handling]
// r[impl dashboard.editing.cancel.discard]
import { useEffect, useRef, useState } from "preact/hooks";
import { html } from "../main";
import { EditorView } from "@codemirror/view";
import { EditorState } from "@codemirror/state";
import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
import { markdown } from "@codemirror/lang-markdown";
import { vim, Vim } from "@replit/codemirror-vim";

interface InlineEditorProps {
  filePath: string;
  byteRange: string; // "start-end"
  onSave: () => void;
  onCancel: () => void;
}

export function InlineEditor({ filePath, byteRange, onSave, onCancel }: InlineEditorProps) {
  const [content, setContent] = useState("");
  const [fileHash, setFileHash] = useState("");
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
        setFileHash(data.file_hash);
        setLoading(false);
      } catch (err) {
        setError(err instanceof Error ? err.message : "Failed to load");
        setLoading(false);
      }
    };
    fetchContent();
  }, [filePath, start, end]);

  // Ref for save handler (so vim commands can access current handler)
  const handleSaveRef = useRef<() => void>(() => {});

  // Initialize CodeMirror when content loads
  useEffect(() => {
    if (!loading && !error && editorRef.current && !editorViewRef.current) {
      // Define vim ex commands for :q (quit) and :wq (write and quit)
      Vim.defineEx("q", "q", () => {
        onCancel();
      });
      Vim.defineEx("wq", "wq", () => {
        handleSaveRef.current();
      });
      Vim.defineEx("w", "w", () => {
        handleSaveRef.current();
      });

      const startState = EditorState.create({
        doc: content,
        extensions: [
          vim(),
          markdown(),
          history(),
          EditorView.lineWrapping,
          EditorView.theme({
            "&": {
              height: "100%",
              fontSize: "0.85rem",
              fontFamily: "var(--font-mono)",
            },
            ".cm-scroller": {
              fontFamily: "var(--font-mono)",
              overflow: "auto",
            },
            ".cm-content": {
              padding: "0.75rem",
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
      view.focus();

      return () => {
        view.destroy();
        editorViewRef.current = null;
      };
    }
  }, [loading, error, content]);

  const handleSave = async () => {
    if (!editorViewRef.current) return;

    const newContent = editorViewRef.current.state.doc.toString();
    setSaving(true);
    setError(null);
    try {
      // r[impl dashboard.editing.api.hash-conflict]
      const response = await fetch("/api/file-range", {
        method: "PATCH",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          path: filePath,
          start,
          end,
          content: newContent,
          file_hash: fileHash,
        }),
      });
      if (!response.ok) {
        if (response.status === 409) {
          throw new Error("File has changed since it was loaded. Please reload and try again.");
        }
        throw new Error("Failed to save");
      }
      const data = await response.json();
      // Update hash for potential future saves
      setFileHash(data.file_hash);
      onSave();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to save");
      setSaving(false);
    }
  };

  // Keep ref updated so vim :wq/:w commands can call handleSave
  handleSaveRef.current = handleSave;

  if (loading) {
    return html`<div class="inline-editor-loading">Loading...</div>`;
  }

  if (error) {
    return html`<div class="inline-editor-error">${error}</div>`;
  }

  return html`
    <div class="inline-editor">
      <div class="inline-editor-header">
        <span class="inline-editor-label">Edit Requirement</span>
        <span class="inline-editor-vim">VIM</span>
        <span class="inline-editor-path">${filePath}</span>
      </div>
      <div class="inline-editor-content">
        <div class="inline-editor-code" ref=${editorRef} />
      </div>
      <div class="inline-editor-footer">
        <button class="inline-editor-btn inline-editor-cancel" onClick=${onCancel} disabled=${saving}>
          Cancel (:q)
        </button>
        <button class="inline-editor-btn inline-editor-save" onClick=${handleSave} disabled=${saving}>
          ${saving ? "Saving..." : "Save (:wq)"}
        </button>
      </div>
    </div>
  `;
}
