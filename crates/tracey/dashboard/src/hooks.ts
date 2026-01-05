// Custom hooks
import { useCallback, useEffect, useState } from "preact/hooks";
import type {
	ApiData,
	Config,
	FileContent,
	ForwardData,
	ReverseData,
	SpecContent,
} from "./types";

async function fetchJson<T>(url: string): Promise<T> {
	const res = await fetch(url);
	if (!res.ok) throw new Error(`HTTP ${res.status}`);
	return res.json();
}

// Parse spec and lang from URL pathname
// URL format: /:spec/:lang/:view/...
function getImplFromUrl(): { spec: string | null; lang: string | null } {
	const parts = window.location.pathname.split("/").filter(Boolean);
	return {
		spec: parts[0] || null,
		lang: parts[1] || null,
	};
}

// Build API URL with spec/lang params
function apiUrl(base: string, spec?: string | null, lang?: string | null): string {
	const params = new URLSearchParams();
	if (spec) params.set("spec", spec);
	if (lang) params.set("lang", lang);
	const query = params.toString();
	return query ? `${base}?${query}` : base;
}

export interface UseApiResult {
	data: ApiData | null;
	error: string | null;
	version: string | null;
	refetch: () => Promise<void>;
}

export function useApi(): UseApiResult {
	const [data, setData] = useState<ApiData | null>(null);
	const [error, setError] = useState<string | null>(null);
	const [version, setVersion] = useState<string | null>(null);

	const fetchData = useCallback(async () => {
		try {
			// First fetch config to get available specs/impls
			const config = await fetchJson<Config>("/api/config");

			// Get spec/lang from URL, falling back to first available
			let { spec, lang } = getImplFromUrl();
			if (!spec && config.specs?.[0]) {
				spec = config.specs[0].name;
			}
			if (!lang && spec) {
				const specInfo = config.specs?.find(s => s.name === spec);
				lang = specInfo?.implementations?.[0] || null;
			}

			// Fetch forward/reverse with spec/lang params
			const [forward, reverse] = await Promise.all([
				fetchJson<ForwardData>(apiUrl("/api/forward", spec, lang)),
				fetchJson<ReverseData>(apiUrl("/api/reverse", spec, lang)),
			]);
			setData({ config, forward, reverse });
			setError(null);
		} catch (e) {
			setError(e instanceof Error ? e.message : String(e));
		}
	}, []);

	// Initial fetch
	useEffect(() => {
		fetchData();
	}, [fetchData]);

	// Refetch when URL changes (spec/lang might change)
	useEffect(() => {
		const handlePopState = () => fetchData();
		window.addEventListener("popstate", handlePopState);
		return () => window.removeEventListener("popstate", handlePopState);
	}, [fetchData]);

	// Poll for version changes and refetch if changed
	useEffect(() => {
		let active = true;
		let lastVersion: string | null = null;

		async function poll() {
			if (!active) return;
			try {
				const res = await fetchJson<{ version: string }>("/api/version");
				if (lastVersion !== null && res.version !== lastVersion) {
					console.log(
						`Version changed: ${lastVersion} -> ${res.version}, refetching...`,
					);
					await fetchData();
				}
				lastVersion = res.version;
				setVersion(res.version);
			} catch (e) {
				console.warn("Version poll failed:", e);
			}
			if (active) setTimeout(poll, 500);
		}

		poll();
		return () => {
			active = false;
		};
	}, [fetchData]);

	return { data, error, version, refetch: fetchData };
}

export function useFile(path: string | null): FileContent | null {
	const [file, setFile] = useState<FileContent | null>(null);

	useEffect(() => {
		if (!path) {
			setFile(null);
			return;
		}
		// Get spec/lang from URL for API call
		const { spec, lang } = getImplFromUrl();
		const params = new URLSearchParams();
		params.set("path", path);
		if (spec) params.set("spec", spec);
		if (lang) params.set("lang", lang);

		fetchJson<FileContent>(`/api/file?${params.toString()}`)
			.then(setFile)
			.catch((e) => {
				console.error("Failed to load file:", e);
				setFile(null);
			});
	}, [path]);

	return file;
}

export function useSpec(
	name: string | null,
	version: string | null,
): SpecContent | null {
	const [spec, setSpec] = useState<SpecContent | null>(null);

	useEffect(() => {
		if (!name) {
			setSpec(null);
			return;
		}
		// Get spec/lang from URL for API call
		const { spec: urlSpec, lang } = getImplFromUrl();
		const params = new URLSearchParams();
		if (urlSpec) params.set("spec", urlSpec);
		if (lang) params.set("lang", lang);

		fetchJson<SpecContent>(`/api/spec?${params.toString()}`)
			.then(setSpec)
			.catch((e) => {
				console.error("Failed to load spec:", e);
				setSpec(null);
			});
	}, [name, version]);

	return spec;
}
