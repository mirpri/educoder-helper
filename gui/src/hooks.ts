import { useCallback, useEffect, useRef, useState } from "react";

import { type ApiError, toApiError } from "./api";

export interface AsyncState<T> {
  data: T | null;
  error: ApiError | null;
  loading: boolean;
  reload: () => void;
}

/**
 * Runs `fn` whenever `deps` change (and on demand via `reload`). Results from a
 * superseded run are dropped, so fast clicking through the browse tree never
 * shows a stale course's homeworks.
 */
export function useAsync<T>(fn: () => Promise<T>, deps: unknown[], enabled = true): AsyncState<T> {
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<ApiError | null>(null);
  const [loading, setLoading] = useState(false);
  const [nonce, setNonce] = useState(0);
  const runId = useRef(0);
  const fnRef = useRef(fn);
  fnRef.current = fn;

  useEffect(() => {
    if (!enabled) {
      setData(null);
      setError(null);
      setLoading(false);
      return;
    }
    const id = ++runId.current;
    setLoading(true);
    setError(null);
    fnRef
      .current()
      .then((value) => {
        if (id !== runId.current) return;
        setData(value);
        setLoading(false);
      })
      .catch((e: unknown) => {
        if (id !== runId.current) return;
        setError(toApiError(e));
        setData(null);
        setLoading(false);
      });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [...deps, enabled, nonce]);

  const reload = useCallback(() => setNonce((n) => n + 1), []);
  return { data, error, loading, reload };
}
