import type { Comparison, CornerComparison } from "./types";

/** Axe horizontal des courbes : position sur la piste (référence calée au même endroit) ou temps écoulé. */
export type Axis = "distance" | "time";
/** Unité de l'écart : secondes de retard ou mètres de retard sur la référence. */
export type GapUnit = "s" | "m";

/** Index du dernier élément ≤ v dans un tableau croissant (−1 si v est avant le début). */
export function floorIndex(xs: readonly number[], v: number): number {
  let lo = 0;
  let hi = xs.length - 1;
  if (hi < 0 || v < xs[0]!) return -1;
  if (v >= xs[hi]!) return hi;
  while (hi - lo > 1) {
    const mid = (lo + hi) >> 1;
    if (xs[mid]! <= v) lo = mid;
    else hi = mid;
  }
  return lo;
}

/** Interpolation linéaire de ys(xs) en v ; `null` hors de l'intervalle couvert. */
export function interp(xs: readonly number[], ys: readonly number[], v: number): number | null {
  const i = floorIndex(xs, v);
  if (i < 0 || i >= xs.length - 1) return i === xs.length - 1 && v === xs[i] ? ys[i]! : null;
  const x0 = xs[i]!;
  const x1 = xs[i + 1]!;
  const t = x1 > x0 ? (v - x0) / (x1 - x0) : 0;
  return ys[i]! + (ys[i + 1]! - ys[i]!) * t;
}

/**
 * Retard en mètres sur la référence, à chaque point de distance du tour analysé : quand tu passes
 * à cet endroit, de combien de mètres la référence est-elle déjà plus loin ? Positif = tu es derrière.
 */
export function gapMeters(c: Comparison): (number | null)[] {
  const r = c.ref_trace;
  const k = c.cmp_trace;
  const rEnd = r.time[r.time.length - 1] ?? 0;
  const vEnd = (r.speed_kmh[r.speed_kmh.length - 1] ?? 0) / 3.6;
  const lastD = r.dist[r.dist.length - 1] ?? 0;
  return k.dist.map((d, i) => {
    const t = k.time[i]!;
    // Référence déjà arrivée : elle continue à sa vitesse de fin de tour.
    const at = t > rEnd ? lastD + (t - rEnd) * vEnd : interp(r.time, r.dist, t);
    return at == null ? null : at - d;
  });
}

export interface CompareView {
  x: number[];
  axis: Axis;
  gap: (number | null)[];
  gapUnit: GapUnit;
  ref: { speed: (number | null)[]; throttle: (number | null)[]; brake: (number | null)[]; gear: (number | null)[] | null; steer: (number | null)[] | null };
  cmp: { speed: (number | null)[]; throttle: (number | null)[]; brake: (number | null)[]; gear: (number | null)[] | null; steer: (number | null)[] | null };
  line: (number | null)[] | null;
  /** Virages : zone entrée → sortie et apex, sur l'axe choisi. */
  bands: { id: number; label: string; from: number; to: number; apex: number }[];
  /** Temps du tour analysé pour chaque abscisse (synchronisation avec la relecture). */
  cmpTime: (number | null)[];
}

/** Nom court pour les graphiques : « Virage 12 » → « V12 », « Virages 14-15 » → « V14-15 ». */
export function shortCorner(name: string): string {
  const m = /^Virages?\s+(.+)$/i.exec(name);
  return m ? `V${m[1]}` : name;
}

const pct = (a: readonly number[]) => a.map((v) => v * 100);

/** Données des courbes selon l'axe et l'unité d'écart. `every` : décimation en mode distance. */
export function buildView(c: Comparison, axis: Axis, gapUnit: GapUnit, every: number, timeStep = 0.05): CompareView {
  const r = c.ref_trace;
  const k = c.cmp_trace;
  const gapAtDist = gapUnit === "s" ? c.delta_s : gapMeters(c);
  const names = c.corners.map((x) => ({ id: x.corner.id, label: shortCorner(x.corner.name), def: x.corner }));
  if (axis === "distance") {
    const idx: number[] = [];
    for (let i = 0; i < r.dist.length; i += every) idx.push(i);
    if (r.dist.length > 0 && idx[idx.length - 1] !== r.dist.length - 1) idx.push(r.dist.length - 1);
    const pick = <T,>(a: readonly T[] | null) => (a ? idx.map((i) => a[i] ?? null) : null);
    return {
      axis,
      gapUnit,
      x: idx.map((i) => r.dist[i]!),
      gap: pick(gapAtDist)!,
      ref: { speed: pick(r.speed_kmh)!, throttle: pick(pct(r.throttle))!, brake: pick(pct(r.brake))!, gear: pick(r.gear), steer: pick(r.steer_deg) },
      cmp: { speed: pick(k.speed_kmh)!, throttle: pick(pct(k.throttle))!, brake: pick(pct(k.brake))!, gear: pick(k.gear), steer: pick(k.steer_deg) },
      line: pick(c.line_offset_m),
      bands: names.map((n) => ({ id: n.id, label: n.label, from: n.def.entry_m, to: n.def.exit_m, apex: n.def.apex_m })),
      cmpTime: idx.map((i) => k.time[i] ?? null),
    };
  }
  // Axe temps : chaque tour est lu à son propre chronomètre ; la référence se détache devant ou derrière.
  const t0 = Math.min(r.time[0] ?? 0, k.time[0] ?? 0);
  const t1 = Math.max(r.time[r.time.length - 1] ?? 0, k.time[k.time.length - 1] ?? 0);
  const x: number[] = [];
  for (let t = t0; t <= t1 + 1e-9; t += timeStep) x.push(Math.round(t * 1000) / 1000);
  const at = (times: readonly number[], ys: readonly number[] | null) => (ys ? x.map((t) => interp(times, ys, t)) : null);
  const kd = at(k.time, k.dist)!;
  const gap = kd.map((d) => (d == null ? null : interp(k.dist, gapAtDist.map((v) => v ?? NaN), d))).map((v) => (v == null || Number.isNaN(v) ? null : v));
  const tOf = (d: number) => interp(k.dist, k.time, d) ?? 0;
  return {
    axis,
    gapUnit,
    x,
    gap,
    ref: { speed: at(r.time, r.speed_kmh)!, throttle: at(r.time, pct(r.throttle))!, brake: at(r.time, pct(r.brake))!, gear: at(r.time, r.gear), steer: at(r.time, r.steer_deg) },
    cmp: { speed: at(k.time, k.speed_kmh)!, throttle: at(k.time, pct(k.throttle))!, brake: at(k.time, pct(k.brake))!, gear: at(k.time, k.gear), steer: at(k.time, k.steer_deg) },
    line: c.line_offset_m ? kd.map((d) => (d == null ? null : interp(k.dist, c.line_offset_m!, d))) : null,
    bands: names.map((n) => ({ id: n.id, label: n.label, from: tOf(n.def.entry_m), to: tOf(n.def.exit_m), apex: tOf(n.def.apex_m) })),
    cmpTime: x.map((t) => (t <= (k.time[k.time.length - 1] ?? 0) ? t : null)),
  };
}

/** Écart final du tour, dans l'unité choisie (positif = retard). */
export function finalGap(c: Comparison, unit: GapUnit): number | null {
  if (unit === "s") return c.delta_s[c.delta_s.length - 1] ?? null;
  // En mètres : où était la référence quand tu as franchi la ligne ?
  const r = c.ref_trace;
  const tEnd = c.cmp_trace.time[c.cmp_trace.time.length - 1];
  const len = c.length_m;
  if (tEnd == null) return null;
  const rEnd = r.time[r.time.length - 1] ?? 0;
  if (tEnd <= rEnd) {
    const d = interp(r.time, r.dist, tEnd);
    return d == null ? null : -(len - d);
  }
  // Référence déjà arrivée : distance parcourue à sa vitesse de fin de tour pendant ton retard.
  const v = (r.speed_kmh[r.speed_kmh.length - 1] ?? 0) / 3.6;
  return (tEnd - rEnd) * v;
}

/** Formate un écart signé : « +0.38 s », « −12 m ». */
export function fmtGap(v: number | null | undefined, unit: GapUnit, digits?: number): string {
  if (v == null || Number.isNaN(v)) return "—";
  const d = digits ?? (unit === "s" ? 3 : 0);
  const r = Math.abs(v) < 0.5 * 10 ** -d ? 0 : v;
  return `${r > 0 ? "+" : r < 0 ? "−" : "±"}${Math.abs(r).toFixed(d)} ${unit}`;
}

export interface CornerFact {
  text: string;
  /** Vrai si ce point te coûte du temps par rapport à la référence. */
  bad: boolean;
}

/**
 * Ce qui différencie ton passage de celui de la référence, en clair, du plus marquant au moins marquant :
 * freinage, vitesse mini, remise des gaz, vitesse de sortie.
 */
export function cornerFacts(k: CornerComparison): CornerFact[] {
  const r = k.reference;
  const c = k.compared;
  const out: { score: number; f: CornerFact }[] = [];
  if (r.brake_on_m != null && c.brake_on_m != null && r.brake_peak >= 0.2 && c.brake_peak >= 0.2) {
    const d = c.brake_on_m - r.brake_on_m;
    if (Math.abs(d) >= 5) out.push({ score: Math.abs(d) / 5, f: { text: `freinage ${Math.abs(d).toFixed(0)} m plus ${d < 0 ? "tôt" : "tard"}`, bad: d < 0 } });
  } else if ((r.brake_peak >= 0.2) !== (c.brake_peak >= 0.2)) {
    out.push({ score: 4, f: c.brake_peak < 0.2 ? { text: "pas de freinage (la réf. freine)", bad: false } : { text: "freinage en plus (la réf. ne freine pas)", bad: true } });
  }
  const dv = c.min_speed_kmh - r.min_speed_kmh;
  if (Math.abs(dv) >= 2) out.push({ score: Math.abs(dv) / 2, f: { text: `${Math.abs(dv).toFixed(0)} km/h ${dv < 0 ? "de moins" : "de plus"} à la corde`, bad: dv < 0 } });
  if (r.throttle_on_m != null && c.throttle_on_m != null) {
    const d = c.throttle_on_m - r.throttle_on_m;
    if (Math.abs(d) >= 8) out.push({ score: Math.abs(d) / 8, f: { text: `gaz ${Math.abs(d).toFixed(0)} m plus ${d > 0 ? "tard" : "tôt"}`, bad: d > 0 } });
  }
  const dx = c.exit_speed_kmh - r.exit_speed_kmh;
  if (Math.abs(dx) >= 3) out.push({ score: Math.abs(dx) / 3, f: { text: `sortie ${Math.abs(dx).toFixed(0)} km/h ${dx < 0 ? "moins vite" : "plus vite"}`, bad: dx < 0 } });
  return out.sort((a, b) => b.score - a.score).map((o) => o.f);
}
