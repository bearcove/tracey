// Router utilities using preact-iso
// [impl dashboard.url.structure] - URL structure: /:spec/:impl/:view
//
// Examples:
//   /rapace/rust/spec                     -> spec view, no heading
//   /rapace/rust/spec#channels            -> spec view, heading "channels" (hash fragment)
//   /rapace/swift/sources                 -> sources view, no file
//   /rapace/rust/sources/src/lib.rs:42    -> sources view, file + line
//   /rapace/rust/coverage                 -> coverage view
//   /rapace/rust/coverage?filter=impl     -> coverage view with filter

export { LocationProvider, Route, Router, useLocation, useRoute } from "preact-iso";

import type { ViewType } from "./types";

export interface UrlParams {
  file?: string | null;
  line?: number | null;
  context?: string | null;
  rule?: string | null;
  heading?: string | null;
  filter?: string | null;
  level?: string | null;
}

export function buildUrl(
  spec: string | null,
  impl: string | null,
  view: ViewType,
  params: UrlParams = {},
): string {
  // Build base path: /{spec}/{impl}
  const specPart = spec ? `/${encodeURIComponent(spec)}` : "";
  const implPart = impl ? `/${encodeURIComponent(impl)}` : "";
  const base = specPart + implPart;

  // [impl dashboard.url.sources-view]
  // [impl dashboard.url.context]
  if (view === "sources") {
    const { file, line, context } = params;
    let url = `${base}/sources`;
    if (file) {
      url = line ? `${base}/sources/${file}:${line}` : `${base}/sources/${file}`;
    }
    if (context) {
      url += `?context=${encodeURIComponent(context)}`;
    }
    return url;
  }

  // [impl dashboard.url.spec-view]
  if (view === "spec") {
    const { rule, heading } = params;
    let url = `${base}/spec`;
    // Requirements use #r--{id} anchors, headings use #{slug}
    if (rule) url += `#r--${rule}`;
    else if (heading) url += `#${heading}`;
    return url;
  }

  // [impl dashboard.url.coverage-view]
  const searchParams = new URLSearchParams();
  if (params.filter) searchParams.set("filter", params.filter);
  if (params.level && params.level !== "all") searchParams.set("level", params.level);
  const query = searchParams.toString();
  return `${base}/coverage${query ? `?${query}` : ""}`;
}
