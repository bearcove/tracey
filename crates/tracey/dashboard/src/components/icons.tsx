// Icon components - LangIcon, LucideIcon

// Declare lucide as global (loaded via CDN)
declare const lucide: { createIcons: (opts?: { nodes?: Node[] }) => void };

// Map file extensions to devicon class names
// See https://devicon.dev/ for available icons
const LANG_DEVICON_MAP = {
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
  sass: "devicon-sass-original",
  sh: "devicon-bash-plain",
  bash: "devicon-bash-plain",
  zsh: "devicon-bash-plain",
  sql: "devicon-postgresql-plain",
  kt: "devicon-kotlin-plain",
  scala: "devicon-scala-plain",
  hs: "devicon-haskell-plain",
  ex: "devicon-elixir-plain",
  exs: "devicon-elixir-plain",
  erl: "devicon-erlang-plain",
  clj: "devicon-clojure-plain",
  php: "devicon-php-plain",
  lua: "devicon-lua-plain",
  r: "devicon-r-plain",
  jl: "devicon-julia-plain",
  dart: "devicon-dart-plain",
  vue: "devicon-vuejs-plain",
  svelte: "devicon-svelte-plain",
  // Default fallback - use Lucide file icon
  default: null,
};

// Get devicon class for a file extension (returns null if no devicon available)
function getDeviconClass(filePath) {
  const ext = filePath.split(".").pop()?.toLowerCase();
  return LANG_DEVICON_MAP[ext] || LANG_DEVICON_MAP.default;
}

// Language icon component - uses devicon if available, falls back to Lucide
function LangIcon({ filePath, className = "" }: LangIconProps) {
  const deviconClass = getDeviconClass(filePath);
  const iconRef = useRef(null);

  // For Lucide fallback
  useEffect(() => {
    if (!deviconClass && iconRef.current && typeof lucide !== "undefined") {
      iconRef.current.innerHTML = "";
      const i = document.createElement("i");
      i.setAttribute("data-lucide", "file");
      iconRef.current.appendChild(i);
      lucide.createIcons({ nodes: [i] });
    }
  }, [deviconClass]);

  if (deviconClass) {
    return html`<i class="${deviconClass} ${className}"></i>`;
  }
  return html`<span ref=${iconRef} class=${className}></span>`;
}

// Create a Lucide icon element (for use in htm templates)
function LucideIcon({ name, className = "" }: LucideIconProps) {
  const iconRef = useRef(null);

  useEffect(() => {
    if (iconRef.current && typeof lucide !== "undefined") {
      iconRef.current.innerHTML = "";
      const i = document.createElement("i");
      i.setAttribute("data-lucide", name);
      iconRef.current.appendChild(i);
      lucide.createIcons({ nodes: [i] });
    }
  }, [name]);

  return html`<span ref=${iconRef} class=${className}></span>`;
}
