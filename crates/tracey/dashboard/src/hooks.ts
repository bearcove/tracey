// Custom hooks
import { useCallback, useEffect, useState } from "preact/hooks";
import type { ApiData, Config, FileContent, ForwardData, ReverseData, SpecContent } from "./types";

async function fetchJson<T>(url: string): Promise<T> {
  const res = await fetch(url);
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();
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
      const [config, forward, reverse] = await Promise.all([
        fetchJson<Config>("/api/config"),
        fetchJson<ForwardData>("/api/forward"),
        fetchJson<ReverseData>("/api/reverse"),
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

  // Poll for version changes and refetch if changed
  useEffect(() => {
    let active = true;
    let lastVersion: string | null = null;

    async function poll() {
      if (!active) return;
      try {
        const res = await fetchJson<{ version: string }>("/api/version");
        if (lastVersion !== null && res.version !== lastVersion) {
          console.log(`Version changed: ${lastVersion} -> ${res.version}, refetching...`);
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
    fetchJson<FileContent>(`/api/file?path=${encodeURIComponent(path)}`)
      .then(setFile)
      .catch((e) => {
        console.error("Failed to load file:", e);
        setFile(null);
      });
  }, [path]);

  return file;
}

export function useSpec(name: string | null, version: string | null): SpecContent | null {
  const [spec, setSpec] = useState<SpecContent | null>(null);

  useEffect(() => {
    if (!name) {
      setSpec(null);
      return;
    }
    fetchJson<SpecContent>(`/api/spec?name=${encodeURIComponent(name)}`)
      .then(setSpec)
      .catch((e) => {
        console.error("Failed to load spec:", e);
        setSpec(null);
      });
  }, [name, version]);

  return spec;
}
