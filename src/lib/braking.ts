// Affichage de la comparaison de freinage (calculée dans apex-core, analysis/braking.rs) : textes
// courts avec leur unité, et les virages prioritaires marqués d'office sur la carte.
import type { BrakeCompare, Comparison, CornerComparison } from "./types";

/** Sous 5 m, c'est le même point (même seuil que apex-core `SAME_M`). */
export const SAME_M = 5;
/** Un virage compte comme priorité au-delà de 0,05 s perdus. */
export const PRIORITY_MIN_S = 0.05;

const fr = (v: number, d = 0) => v.toFixed(d).replace(".", ",");

/** « 18 m trop tôt », « 6 m plus tard », « au même endroit » ; null si l'écart n'est pas mesuré. */
export function brakeGapText(b: BrakeCompare): string | null {
  const g = b.brake_gap_m;
  if (g == null) return null;
  if (Math.abs(g) < SAME_M) return "au même endroit";
  return g < 0 ? `${fr(-g)} m trop tôt` : `${fr(g)} m plus tard`;
}

/** « gaz 12 m plus tard » / « gaz 8 m plus tôt » ; null sans écart mesurable. */
export function throttleGapText(b: BrakeCompare): string | null {
  const g = b.throttle_gap_m;
  if (g == null || Math.abs(g) < SAME_M) return null;
  return g > 0 ? `gaz ${fr(g)} m plus tard` : `gaz ${fr(-g)} m plus tôt`;
}

/** « −4 km/h à la corde » ; null sous 1 km/h. */
export function minSpeedText(b: BrakeCompare): string | null {
  const v = b.min_speed_gap_kmh;
  if (Math.abs(v) < 1) return null;
  return `${v > 0 ? "+" : "−"}${fr(Math.abs(v))} km/h à la corde`;
}

/** Perte d'un virage en secondes, signée : « +0,32 s ». */
export function lossText(s: number): string {
  return `${s >= 0 ? "+" : "−"}${fr(Math.abs(s), 2)} s`;
}

/** Les 3 virages où le tour perd le plus (au-delà de 0,05 s), dans l'ordre de la perte. */
export function priorityCorners(c: Comparison, n = 3): CornerComparison[] {
  return [...c.corners].filter((k) => k.time_delta_s > PRIORITY_MIN_S).sort((a, b) => b.time_delta_s - a.time_delta_s).slice(0, n);
}
