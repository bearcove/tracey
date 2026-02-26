// Configuration constants
import type { Editor } from "./types";

// Editor configurations with devicon classes (zed uses inline SVG since devicon font doesn't have it yet)
const ZED_SVG = `<svg class="editor-icon-svg" viewBox="0 0 128 128"><path fill="currentColor" d="M12 8a4 4 0 0 0-4 4v88H0V12C0 5.373 5.373 0 12 0h107.172c5.345 0 8.022 6.463 4.242 10.243L57.407 76.25H76V68h8v10.028a4 4 0 0 1-4 4H49.97l-13.727 13.729H98V56h8v47.757a8 8 0 0 1-8 8H27.657l-13.97 13.97H116a4 4 0 0 0 4-4V28h8v93.757c0 6.627-5.373 12-12 12H8.828c-5.345 0-8.022-6.463-4.242-10.243L70.343 57.757H52v8h-8V55.728a4 4 0 0 1 4-4h30.086l13.727-13.728H30V78h-8V30.243a8 8 0 0 1 8-8h70.343l13.97-13.971H12z"/></svg>`;

export const EDITORS: Record<string, Editor> = {
  zed: {
    name: "Zed",
    urlTemplate: (path, line) => `zed://file/${path}:${line}`,
    icon: ZED_SVG,
  },
  vscode: {
    name: "VS Code",
    urlTemplate: (path, line) => `vscode://file/${path}:${line}`,
    devicon: "devicon-vscode-plain",
  },
  idea: {
    name: "IntelliJ",
    urlTemplate: (path, line) => `idea://open?file=${path}&line=${line}`,
    devicon: "devicon-intellij-plain",
  },
  vim: {
    name: "Vim",
    urlTemplate: (path, line) => `mvim://open?url=file://${path}&line=${line}`,
    devicon: "devicon-vim-plain",
  },
  neovim: {
    name: "Neovim",
    urlTemplate: (path, line) => `nvim://open?file=${path}&line=${line}`,
    devicon: "devicon-neovim-plain",
  },
  emacs: {
    name: "Emacs",
    urlTemplate: (path, line) => `emacs://open?url=file://${path}&line=${line}`,
    devicon: "devicon-emacs-original",
  },
};

export const LEVELS: Record<string, { name: string; dotClass: string }> = {
  all: { name: "All", dotClass: "level-dot-all" },
  must: { name: "MUST", dotClass: "level-dot-must" },
  should: { name: "SHOULD", dotClass: "level-dot-should" },
  may: { name: "MAY", dotClass: "level-dot-may" },
};

// Map file extensions to devicon class names
// See https://devicon.dev/ for available icons
export const LANG_DEVICON_MAP: Record<string, string> = {
  rs: "devicon-rust-original",
  ts: "devicon-typescript-plain",
  tsx: "devicon-typescript-plain",
  js: "devicon-javascript-plain",
  jsx: "devicon-javascript-plain",
  py: "devicon-python-plain",
  go: "devicon-go-plain",
  c: "devicon-c-plain",
  cpp: "devicon-cplusplus-plain",
  h: "devicon-c-plain",
  hpp: "devicon-cplusplus-plain",
  swift: "devicon-swift-plain",
  java: "devicon-java-plain",
  rb: "devicon-ruby-plain",
  md: "devicon-markdown-original",
  json: "devicon-json-plain",
  yaml: "devicon-yaml-plain",
  yml: "devicon-yaml-plain",
  toml: "devicon-toml-plain",
  html: "devicon-html5-plain",
  css: "devicon-css3-plain",
  scss: "devicon-sass-original",
  vue: "devicon-vuejs-plain",
  svelte: "devicon-svelte-plain",
  php: "devicon-php-plain",
  kt: "devicon-kotlin-plain",
  scala: "devicon-scala-plain",
  zig: "devicon-zig-original",
  lua: "devicon-lua-plain",
};

// Tab icon names (Lucide)
export const TAB_ICON_NAMES: Record<string, string> = {
  specification: "file-text",
  coverage: "bar-chart-3",
  sources: "folder-open",
};

export const SIDEBAR_COLLAPSED_STORAGE_KEY = "tracey.sidebar.collapsed";

// Detect platform for keyboard shortcuts
export const isMac =
  typeof navigator !== "undefined" && navigator.platform.toUpperCase().indexOf("MAC") >= 0;
export const modKey = isMac ? "⌘" : "Ctrl";

// Get devicon class for a file extension (returns null if no devicon available)
export function getDeviconClass(filePath: string): string | null {
  const ext = filePath.split(".").pop()?.toLowerCase();
  return (ext && LANG_DEVICON_MAP[ext]) || null;
}
