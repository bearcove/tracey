import { useEffect, useRef, useState } from "preact/hooks";
import { html } from "../main";
import { EditorView } from "@codemirror/view";
import { EditorState } from "@codemirror/state";
import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
import { markdown } from "@codemirror/lang-markdown";
import { vim } from "@replit/codemirror-vim";

interface InlineEditorProps {
  filePath: string;
  byteRange: string; // "start-end"
  onSave: () => void;
  onCancel: () => void;
}

export function InlineEditor({ filePath, byteRange, onSave, onCancel }: InlineEditorProps) {
  const [content, setContent] = useState("");
  const [preview, setPreview] = useState("");
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const editorRef = useRef<HTMLDivElement>(null);
  const editorViewRef = useRef<EditorView | null>(null);
  const debounceTimerRef = useRef<number | null>(null);

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
        // Initial preview
        updatePreview(data.content);
      } catch (err) {
        setError(err instanceof Error ? err.message : "Failed to load");
        setLoading(false);
      }
    };
    fetchContent();
  }, [filePath, start, end]);

  // Update preview (debounced)
  const updatePreview = async (text: string) => {
    try {
      const response = await fetch("/api/preview-markdown", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ content: text }),
      });
      if (response.ok) {
        const data = await response.json();
        setPreview(data.html);
      }
    } catch (err) {
      console.error("Preview error:", err);
    }
  };

  // Initialize CodeMirror when content loads
  useEffect(() => {
    if (!loading && !error && editorRef.current && !editorViewRef.current) {
      const startState = EditorState.create({
        doc: content,
        extensions: [
          vim(),
          markdown(),
          history(),
          EditorView.updateListener.of((update) => {
            if (update.docChanged) {
              const newContent = update.state.doc.toString();

              // Debounce preview updates
              if (debounceTimerRef.current !== null) {
                clearTimeout(debounceTimerRef.current);
              }
              debounceTimerRef.current = window.setTimeout(() => {
                updatePreview(newContent);
              }, 300);
            }
          }),
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
        if (debounceTimerRef.current !== null) {
          clearTimeout(debounceTimerRef.current);
        }
      };
    }
  }, [loading, error, content]);

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
      onSave();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to save");
      setSaving(false);
    }
  };

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
        <div class="inline-editor-pane">
          <div class="inline-editor-pane-header">Source</div>
          <div class="inline-editor-code" ref=${editorRef} />
        </div>
        <div class="inline-editor-pane">
          <div class="inline-editor-pane-header">Preview</div>
          <div class="inline-editor-preview markdown" dangerouslySetInnerHTML=${{ __html: preview }} />
        </div>
      </div>
      <div class="inline-editor-footer">
        <button class="inline-editor-btn inline-editor-cancel" onClick=${onCancel} disabled=${saving}>
          Cancel (Esc)
        </button>
        <button class="inline-editor-btn inline-editor-save" onClick=${handleSave} disabled=${saving}>
          ${saving ? "Saving..." : "Save"}
        </button>
      </div>
    </div>
  `;
}
