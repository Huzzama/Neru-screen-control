import { useCallback, useEffect, useRef, useState } from "react";

import { api, type AppConfig, type EngineStatus, type PreviewFrame, type Snapshot } from "./api";

const EMPTY_STATUS: EngineStatus = {
  connected: false,
  screen: null,
  detected: [],
  needs_udev: false,
  last_error: null,
  paused: false,
  frames: 0,
  fps_actual: 0,
  render_ms: 0,
  send_ms: 0,
  theme_name: "",
  theme_width: 720,
  theme_height: 720,
  media_errors: [],
};

/** Engine status, pushed by the backend every second. */
export function useStatus(): EngineStatus {
  const [status, setStatus] = useState<EngineStatus>(EMPTY_STATUS);
  useEffect(() => {
    let alive = true;
    api.status().then((s) => alive && setStatus(s)).catch(() => {});
    const un = api.onStatus((s) => alive && setStatus(s));
    return () => {
      alive = false;
      un.then((f) => f()).catch(() => {});
    };
  }, []);
  return status;
}

/** Metrics polled every `ms`. */
export function useMetrics(ms = 1000): Snapshot | null {
  const [snap, setSnap] = useState<Snapshot | null>(null);
  useEffect(() => {
    let alive = true;
    const tick = () => api.metrics().then((s) => alive && setSnap(s)).catch(() => {});
    tick();
    const t = setInterval(tick, ms);
    return () => {
      alive = false;
      clearInterval(t);
    };
  }, [ms]);
  return snap;
}

/** Live preview frames (data URL). Turns the engine's preview on while mounted. */
export function usePreview(enabled = true): PreviewFrame | null {
  const [frame, setFrame] = useState<PreviewFrame | null>(null);
  useEffect(() => {
    if (!enabled) return;
    let alive = true;
    api.setPreview(true).catch(() => {});
    const un = api.onPreview((f) => alive && setFrame(f));
    return () => {
      alive = false;
      api.setPreview(false).catch(() => {});
      un.then((f) => f()).catch(() => {});
    };
  }, [enabled]);
  return frame;
}

export function useConfig(): [AppConfig | null, (patch: (c: AppConfig) => AppConfig) => Promise<void>, () => Promise<void>] {
  const [config, setConfig] = useState<AppConfig | null>(null);
  const reload = useCallback(async () => {
    setConfig(await api.config());
  }, []);
  useEffect(() => {
    reload().catch(() => {});
  }, [reload]);
  const update = useCallback(
    async (patch: (c: AppConfig) => AppConfig) => {
      const base = config ?? (await api.config());
      const next = patch(structuredClone(base));
      setConfig(next);
      await api.setConfig(next);
    },
    [config],
  );
  return [config, update, reload];
}

/** Debounced callback that always sees the latest value. */
export function useDebounced<T>(value: T, ms: number, cb: (v: T) => void) {
  const cbRef = useRef(cb);
  cbRef.current = cb;
  useEffect(() => {
    const t = setTimeout(() => cbRef.current(value), ms);
    return () => clearTimeout(t);
  }, [value, ms]);
}

export function useAsync<T>(fn: () => Promise<T>, deps: unknown[]): [T | null, boolean, string | null, () => void] {
  const [data, setData] = useState<T | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [n, setN] = useState(0);
  useEffect(() => {
    let alive = true;
    setLoading(true);
    fn()
      .then((d) => {
        if (alive) {
          setData(d);
          setError(null);
        }
      })
      .catch((e) => alive && setError(String(e)))
      .finally(() => alive && setLoading(false));
    return () => {
      alive = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [...deps, n]);
  return [data, loading, error, () => setN((x) => x + 1)];
}
