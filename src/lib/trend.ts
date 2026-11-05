import type { HistoryLap } from "./types";

/** Indicateurs d'une séance, calculés sur ses tours lancés (hors sorties et entrées de stands). */
export interface SessionPoint {
  session_id: number;
  at: number;
  timed: number;
  clean: number;
  /** Part des tours lancés restés propres (0..1). */
  clean_rate: number;
  /** Écart-type des tours propres (s), s'il y en a au moins 3. */
  spread_s: number | null;
  /** Meilleur tour propre (s). */
  best_s: number | null;
}

export function sessionTrend(laps: HistoryLap[]): SessionPoint[] {
  const by = new Map<number, HistoryLap[]>();
  for (const l of laps) {
    if (l.kind !== "flying" || l.lap_time_s == null) continue;
    const a = by.get(l.session_id) ?? [];
    a.push(l);
    by.set(l.session_id, a);
  }
  const out: SessionPoint[] = [];
  for (const [id, ls] of by) {
    const clean = ls.filter((l) => l.clean).map((l) => l.lap_time_s!);
    let spread: number | null = null;
    if (clean.length >= 3) {
      const m = clean.reduce((a, b) => a + b, 0) / clean.length;
      spread = Math.sqrt(clean.reduce((a, b) => a + (b - m) ** 2, 0) / (clean.length - 1));
    }
    out.push({
      session_id: id,
      at: ls[0]!.started_at_unix,
      timed: ls.length,
      clean: clean.length,
      clean_rate: clean.length / ls.length,
      spread_s: spread,
      best_s: clean.length ? Math.min(...clean) : null,
    });
  }
  return out.sort((a, b) => a.at - b.at);
}

/** Tendance sur les dernières séances : compare la moyenne des 3 dernières aux 3 d'avant. */
export function direction(values: (number | null)[], higherIsBetter: boolean): "better" | "worse" | "flat" | null {
  const v = values.filter((x): x is number => x != null);
  if (v.length < 4) return null;
  const last = v.slice(-3), prev = v.slice(-6, -3);
  const avg = (a: number[]) => a.reduce((x, y) => x + y, 0) / a.length;
  const d = avg(last) - avg(prev);
  const tol = Math.abs(avg(prev)) * 0.02;
  if (Math.abs(d) <= tol) return "flat";
  return (d > 0) === higherIsBetter ? "better" : "worse";
}
