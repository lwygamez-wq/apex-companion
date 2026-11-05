import { useEffect, useRef, useState } from "react";
import { api, onSessionsChanged } from "../lib/api";
import { delta, lapTime } from "../lib/format";
import type { SessionSummary } from "../lib/types";

/** Bilan express d'une séance qui vient d'être importée (par rapport à ton record d'avant sur le combo). */
export function bilan(s: SessionSummary, all: SessionSummary[]): string {
  const before = all.filter((x) => x.id !== s.id && x.track === s.track && x.track_config === s.track_config && x.car === s.car && x.started_at_unix < s.started_at_unix && x.best_lap_s != null);
  const prev = before.length ? Math.min(...before.map((x) => x.best_lap_s!)) : null;
  const parts = [`${s.clean_lap_count}/${s.lap_count} tours propres`];
  if (s.best_lap_s != null) {
    parts.push(
      prev == null ? `meilleur tour ${lapTime(s.best_lap_s)} (premier record ici)`
        : s.best_lap_s < prev ? `nouveau record ${lapTime(s.best_lap_s)} (${delta(s.best_lap_s - prev)} s)`
        : `meilleur tour ${lapTime(s.best_lap_s)} (${delta(s.best_lap_s - prev)} s du record)`,
    );
  }
  return parts.join(" · ");
}

/**
 * Juste après une séance : un bandeau « séance importée » avec son bilan et un accès direct.
 * N'apparaît que pour une séance arrivée pendant que l'app est ouverte.
 */
export function NewSessionBanner({ onOpen }: { onOpen: (id: number) => void }) {
  const known = useRef<number | null>(null);
  const [fresh, setFresh] = useState<{ s: SessionSummary; text: string } | null>(null);
  useEffect(() => {
    const check = () =>
      api.sessions().then((all) => {
        const top = all.reduce<SessionSummary | null>((m, x) => (m == null || x.id > m.id ? x : m), null);
        if (!top) return;
        if (known.current != null && top.id > known.current && top.lap_count > 0) setFresh({ s: top, text: bilan(top, all) });
        known.current = Math.max(known.current ?? 0, top.id);
      }, () => undefined);
    void check();
    const un = onSessionsChanged(() => void check());
    return () => void un.then((f) => f());
  }, []);
  if (!fresh) return null;
  return (
    <div className="card notice fresh">
      <div>
        <b>Séance importée : {fresh.s.track}</b>
        <div className="muted">{fresh.text}</div>
      </div>
      <div className="row">
        <button className="btn primary" onClick={() => (onOpen(fresh.s.id), setFresh(null))}>Voir le bilan</button>
        <button className="btn ghost small" onClick={() => setFresh(null)}>Plus tard</button>
      </div>
    </div>
  );
}
