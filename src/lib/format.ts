import type { Confidence, LapKind, Priority, SessionKind, Support } from "./types";

/** 83.456 → « 1:23.456 » ; null → « — ». */
export function lapTime(s: number | null | undefined): string {
  if (s == null || !Number.isFinite(s) || s < 0) return "—";
  const totalMs = Math.round(s * 1000);
  const m = Math.floor(totalMs / 60000);
  const rest = (totalMs - m * 60000) / 1000;
  const sec = rest.toFixed(3).padStart(6, "0");
  return m > 0 ? `${m}:${sec}` : rest.toFixed(3);
}

/** Écart signé : « +0.182 » / « −0.050 ». */
export function delta(s: number | null | undefined, digits = 3): string {
  if (s == null || !Number.isFinite(s)) return "—";
  const eps = 0.5 * 10 ** -digits;
  if (Math.abs(s) < eps) return `±${(0).toFixed(digits)}`;
  return s > 0 ? `+${s.toFixed(digits)}` : `−${Math.abs(s).toFixed(digits)}`;
}

export function speed(kmh: number, units: "metric" | "imperial" = "metric"): string {
  return units === "metric" ? `${kmh.toFixed(0)} km/h` : `${(kmh / 1.609344).toFixed(0)} mph`;
}

export function dateTime(unix: number): string {
  return new Date(unix * 1000).toLocaleString("fr-FR", { dateStyle: "medium", timeStyle: "short" });
}

export const SESSION_KIND_FR: Record<SessionKind, string> = {
  practice: "Essais libres",
  qualifying: "Qualification",
  race: "Course",
  hotlap: "Hotlap",
  private_testing: "Entraînement privé",
  unknown: "Type inconnu",
};

export const LAP_KIND_FR: Record<LapKind, string> = {
  out_lap: "Tour de sortie",
  flying: "Tour lancé",
  in_lap: "Retour stands",
  incomplete: "Incomplet",
};

export const PRIORITY_FR: Record<Priority, string> = {
  critical: "Critique",
  important: "Important",
  improvement: "Amélioration possible",
  info: "Information",
};

export const CONFIDENCE_FR: Record<Confidence, string> = { low: "faible", medium: "moyenne", high: "élevée" };

export const SUPPORT_FR: Record<Support, string> = {
  implemented: "Disponible",
  unverified: "Non validé",
  planned: "Prévu",
  unavailable: "Non disponible",
};

/** Sous-échantillonne un tableau pour l'affichage (garde le dernier point). */
export function decimate<T>(arr: readonly T[], every: number): T[] {
  if (every <= 1) return arr.slice();
  const out: T[] = [];
  for (let i = 0; i < arr.length; i += every) out.push(arr[i] as T);
  if (arr.length > 0 && (arr.length - 1) % every !== 0) out.push(arr[arr.length - 1] as T);
  return out;
}
