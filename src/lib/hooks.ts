import { useCallback, useEffect, useState } from "react";
import { api, onCoach, onStatus } from "./api";
import { speak } from "./voice";
import type { StatusPayload } from "./types";

export interface Async<T> {
  data: T | null;
  error: string | null;
  loading: boolean;
  reload: () => void;
}

/** Charge une ressource asynchrone ; `deps` relance le chargement. */
export function useAsync<T>(fn: () => Promise<T>, deps: unknown[]): Async<T> {
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [tick, setTick] = useState(0);
  const reload = useCallback(() => setTick((t) => t + 1), []);
  useEffect(() => {
    let alive = true;
    setLoading(true);
    fn()
      .then((d) => alive && (setData(d), setError(null)))
      .catch((e: unknown) => alive && setError(e instanceof Error ? e.message : String(e)))
      .finally(() => alive && setLoading(false));
    return () => {
      alive = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [...deps, tick]);
  return { data, error, loading, reload };
}

/** Voix du coach : active tant que l'application est ouverte, quelle que soit la page affichée. */
export function useCoachVoice(): void {
  useEffect(() => {
    const un = onCoach((e) => {
      if (e.speak) speak(e.message.text);
    });
    return () => {
      void un.then((f) => f());
    };
  }, []);
}

export function useStatus(): StatusPayload | null {
  const [s, setS] = useState<StatusPayload | null>(null);
  useEffect(() => {
    api.status().then(setS).catch(() => undefined);
    const un = onStatus(setS);
    return () => {
      void un.then((f) => f());
    };
  }, []);
  return s;
}
