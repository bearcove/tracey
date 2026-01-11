// Custom hooks
import { useCallback, useEffect, useState } from "preact/hooks";
import type {
  ApiData,
  Config,
  FileContent,
  ForwardData,
  HealthData,
  ReverseData,
  SpecContent,
} from "./types";

export class ApiError extends Error {
  constructor(
    public status: number,
    public code: string,
    message: string,
  ) {
    super(message);
    this.name = "ApiError";
  }

  get isNoConfig(): boolean {
    return this.status === 404 && this.code === "no_specs";
  }
}

async function fetchJson<T>(url: string): Promise<T> {
  const res = await fetch(url);
  if (!res.ok) {
    // Try to parse error response
    try {
      const body = await res.json();
      throw new ApiError(res.status, body.code || "unknown", body.error || `HTTP ${res.status}`);
    } catch (e) {
      if (e instanceof ApiError) throw e;
      throw new ApiError(res.status, "unknown", `HTTP ${res.status}`);
    }
  }
  return res.json();
}

// Parse spec and impl from URL pathname
// URL format: /:spec/:impl/:view/...
function getImplFromUrl(): { spec: string | null; impl: string | null } {
  const parts = window.location.pathname.split("/").filter(Boolean);
  return {
    spec: parts[0] || null,
    impl: parts[1] || null,
  };
}

// Build API URL with spec/impl params
function apiUrl(base: string, spec?: string | null, impl?: string | null): string {
  const params = new URLSearchParams();
  if (spec) params.set("spec", spec);
  if (impl) params.set("impl", impl);
  const query = params.toString();
  return query ? `${base}?${query}` : base;
}

export interface UseApiResult {
  data: ApiData | null;
  error: string | null;
  version: string | null;
  configError: string | null;
  refetch: () => Promise<void>;
}

export function useApi(): UseApiResult {
  const [data, setData] = useState<ApiData | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [version, setVersion] = useState<string | null>(null);
  const [configError, setConfigError] = useState<string | null>(null);
  // Track current URL path to detect spec/impl changes
  const [urlPath, setUrlPath] = useState(window.location.pathname);

  const fetchData = useCallback(async () => {
    try {
      // Fetch config and health in parallel
      const [config, health] = await Promise.all([
        fetchJson<Config>("/api/config"),
        fetchJson<HealthData>("/api/health").catch(() => null),
      ]);

      // Update config error state
      setConfigError(health?.configError || null);

      // If there are no specs configured, show empty state with config error
      if (!config.specs?.length) {
        setData({
          config,
          forward: { specs: [] },
          reverse: { files: [], totalUnits: 0, coveredUnits: 0 },
        });
        setError(null);
        return;
      }

      // Get spec/impl from URL, falling back to first available
      let { spec, impl } = getImplFromUrl();
      if (!spec && config.specs?.[0]) {
        spec = config.specs[0].name;
      }
      if (!impl && spec) {
        const specInfo = config.specs?.find((s) => s.name === spec);
        impl = specInfo?.implementations?.[0] || null;
      }

      // Fetch forward/reverse with spec/impl params
      const [forward, reverse] = await Promise.all([
        fetchJson<ForwardData>(apiUrl("/api/forward", spec, impl)),
        fetchJson<ReverseData>(apiUrl("/api/reverse", spec, impl)),
      ]);
      setData({ config, forward, reverse });
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, []);

  // Refetch when URL path changes (spec/impl might change)
  useEffect(() => {
    fetchData();
  }, [fetchData, urlPath]);

  // Listen for URL changes from both popstate and programmatic navigation
  useEffect(() => {
    const checkUrlChange = () => {
      const newPath = window.location.pathname;
      if (newPath !== urlPath) {
        setUrlPath(newPath);
      }
    };

    // popstate fires on back/forward
    window.addEventListener("popstate", checkUrlChange);

    // For programmatic navigation, we need to poll or use MutationObserver
    // preact-iso uses history.pushState which doesn't fire popstate
    // Use a short interval to detect changes
    const interval = setInterval(checkUrlChange, 100);

    return () => {
      window.removeEventListener("popstate", checkUrlChange);
      clearInterval(interval);
    };
  }, [urlPath]);

  // r[impl dashboard.api.version]
  // r[impl dashboard.api.live-updates]
  // r[impl dashboard.editing.reload.auto-detect]
  // r[impl dashboard.editing.reload.live-update]
  // Connect to WebSocket for live version updates
  useEffect(() => {
    let ws: WebSocket | null = null;
    let reconnectTimeout: ReturnType<typeof setTimeout> | null = null;
    let lastVersion: number | null = null;

    function connect() {
      const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
      const wsUrl = `${protocol}//${window.location.host}/ws`;

      ws = new WebSocket(wsUrl);

      ws.onopen = () => {
        console.log("WebSocket connected");
      };

      ws.onmessage = async (event) => {
        try {
          const msg = JSON.parse(event.data);
          if (msg.type === "version") {
            const newVersion = msg.version;
            if (lastVersion !== null && newVersion !== lastVersion) {
              console.log(`Version changed: ${lastVersion} -> ${newVersion}, refetching...`);
              await fetchData();
            }
            lastVersion = newVersion;
            setVersion(String(newVersion));
          }
        } catch (e) {
          console.warn("WebSocket message parse error:", e);
        }
      };

      ws.onclose = () => {
        console.log("WebSocket disconnected, reconnecting in 2s...");
        // Reconnect after delay
        reconnectTimeout = setTimeout(connect, 2000);
      };

      ws.onerror = (e) => {
        console.warn("WebSocket error:", e);
        ws?.close();
      };
    }

    connect();

    return () => {
      if (reconnectTimeout) clearTimeout(reconnectTimeout);
      if (ws) {
        ws.onclose = null; // Prevent reconnect on intentional close
        ws.close();
      }
    };
  }, [fetchData]);

  return { data, error, version, configError, refetch: fetchData };
}

export function useFile(path: string | null): FileContent | null {
  const [file, setFile] = useState<FileContent | null>(null);

  useEffect(() => {
    if (!path) {
      setFile(null);
      return;
    }
    // Get spec/impl from URL for API call
    const { spec, impl } = getImplFromUrl();
    const params = new URLSearchParams();
    params.set("path", path);
    if (spec) params.set("spec", spec);
    if (impl) params.set("impl", impl);

    fetchJson<FileContent>(`/api/file?${params.toString()}`)
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
    // Get spec/impl from URL for API call
    const { spec: urlSpec, impl } = getImplFromUrl();
    const params = new URLSearchParams();
    if (urlSpec) params.set("spec", urlSpec);
    if (impl) params.set("impl", impl);

    fetchJson<SpecContent>(`/api/spec?${params.toString()}`)
      .then(setSpec)
      .catch((e) => {
        console.error("Failed to load spec:", e);
        setSpec(null);
      });
  }, [name, version]);

  return spec;
}
