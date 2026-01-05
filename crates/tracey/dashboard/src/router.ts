// Router utilities using preact-iso
// URL structure: /:specName/:view/...
//
// Examples:
//   /rapace/spec                     -> spec view, no heading
//   /rapace/spec/channels            -> spec view, heading "channels"
//   /rapace/sources                  -> sources view, no file
//   /rapace/sources/src/lib.rs:42    -> sources view, file + line
//   /rapace/coverage                 -> coverage view
//   /rapace/coverage?filter=impl     -> coverage view with filter

export {
	LocationProvider,
	Route,
	Router,
	useLocation,
	useRoute,
} from "preact-iso";

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
	view: ViewType,
	params: UrlParams = {},
): string {
	const base = spec ? `/${encodeURIComponent(spec)}` : "";

	if (view === "sources") {
		const { file, line, context } = params;
		let url = `${base}/sources`;
		if (file) {
			url = line
				? `${base}/sources/${file}:${line}`
				: `${base}/sources/${file}`;
		}
		if (context) {
			url += `?context=${encodeURIComponent(context)}`;
		}
		return url;
	}

	if (view === "spec") {
		const { rule, heading } = params;
		let url = `${base}/spec`;
		if (heading) url += `/${heading}`;
		if (rule) url += `?rule=${encodeURIComponent(rule)}`;
		return url;
	}

	// coverage
	const searchParams = new URLSearchParams();
	if (params.filter) searchParams.set("filter", params.filter);
	if (params.level && params.level !== "all")
		searchParams.set("level", params.level);
	const query = searchParams.toString();
	return `${base}/coverage${query ? `?${query}` : ""}`;
}
