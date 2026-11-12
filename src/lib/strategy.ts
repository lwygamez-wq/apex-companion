import type { FuelUnit, PlanOption, Stint } from "./types";

const n2 = (v: number) => v.toFixed(2).replace(".", ",");
/** Niveau à régler, arrondi au-dessus sans que le bruit de calcul ajoute 1 (même règle que le Rust). */
export const up = (v: number) => Math.ceil(v - 0.05);

// ---------------------------------------------------------------------------------------------
// Convertisseur : tours ↔ énergie virtuelle (%) ↔ carburant (l), avec le ratio carburant.
// LMU : au stand on règle l'énergie et le ratio (litres par 1 % d'énergie) ; carburant = énergie × ratio.
// ---------------------------------------------------------------------------------------------

export type ConvField = "laps" | "energy" | "litres";

export interface ConvInput {
  /** Champ saisi en dernier : les deux autres en découlent. */
  from: ConvField;
  value: number;
  ratio: number;
  /** Consommation d'énergie par tour (%) ; sans elle, pas de tours. */
  perLap: number | null;
  /** Marge gardée, en tours de consommation (même marge que le plan). */
  marginLaps: number;
  /** Réservoir de carburant (l), s'il est connu. */
  tankL: number | null;
}

export interface ConvResult {
  energy: number;
  litres: number;
  /** Tours que tient ce niveau, marge gardée ; `null` sans consommation. */
  laps: number | null;
  /** Énergie au-delà de 100 % : impossible, plafonnée. */
  overFull: boolean;
  /** Carburant au-delà du réservoir : c'est le carburant qui limite. */
  overTank: boolean;
  /** Énergie maximale réglable avec ce ratio et ce réservoir. */
  maxEnergy: number;
}

export function convert(i: ConvInput): ConvResult | null {
  const r = i.ratio;
  const per = i.perLap != null && i.perLap > 0 ? i.perLap : null;
  if (!(r > 0) || !(i.value >= 0)) return null;
  let energy: number;
  if (i.from === "energy") energy = i.value;
  else if (i.from === "litres") energy = i.value / r;
  else {
    if (per == null) return null;
    energy = (i.value + i.marginLaps) * per;
  }
  const maxEnergy = Math.min(100, i.tankL != null && i.tankL > 0 ? i.tankL / r : 100);
  const litres = energy * r;
  const laps = per != null ? Math.max(0, energy / per - i.marginLaps) : null;
  return { energy, litres, laps, overFull: energy > 100 + 1e-6, overTank: i.tankL != null && litres > i.tankL + 1e-6, maxEnergy };
}

// ---------------------------------------------------------------------------------------------
// Consigne : la stratégie en trois lignes (quoi mettre, sous quelle barre rester, quand rentrer).
// ---------------------------------------------------------------------------------------------

export interface PitCall {
  /** Rentrer à la fin de ce tour. */
  boxLap: number;
  /** Niveau à régler (arrondi au-dessus) et carburant correspondant. */
  set: number;
  litres: number | null;
}

export interface Consigne {
  start: { set: number; litres: number | null };
  /** Barre de consommation par tour à ne pas dépasser. */
  bar: number;
  /** Économiser (rester sous la barre) ou non (la barre laisse de la marge). */
  mustSave: boolean;
  /** Économie demandée par rapport à ta consommation (part), plan économie. */
  saving: number | null;
  stops: PitCall[];
  /** Pourquoi ce plan, en une phrase. */
  why: string;
}

const stintSet = (u: FuelUnit, st: Stint, ratio: number | null) => {
  const set = up(st.amount);
  return { set, litres: u === "energy" && ratio != null ? Math.round(set * ratio) : null };
};

export function consigne(u: FuelUnit, chosen: PlanOption, options: PlanOption[], perLap: number, ratio: number | null): Consigne | null {
  const first = chosen.stints[0];
  if (!first) return null;
  const save = chosen.kind === "save";
  const attack = options.find((o) => o.kind === "attack");
  const eco = options.find((o) => o.kind === "save");
  const unit = u === "energy" ? "%" : "L";
  let why: string;
  if (save && attack) {
    why = `${chosen.stops} arrêt${chosen.stops > 1 ? "s" : ""} au lieu de ${attack.stops}, contre ${Math.round((chosen.saving ?? 0) * 100)} % d'économie : lift and coast avant les gros freinages.`;
  } else if (chosen.stops === 0) {
    why = `Sans arrêt : il te reste de la marge sur toute la course.`;
  } else if (eco) {
    why = `Un arrêt de moins demanderait de rester sous ${n2(eco.target_per_lap)} ${unit} par tour (${Math.round((eco.saving ?? 0) * 100)} % d'économie) : choisis le plan Économie si tu t'en sens capable.`;
  } else {
    why = `Un arrêt de moins demanderait plus de 8 % d'économie : pas tenable, on n'économise pas.`;
  }
  return {
    start: stintSet(u, first, ratio),
    bar: chosen.target_per_lap,
    mustSave: chosen.target_per_lap < perLap - 1e-6,
    saving: chosen.saving,
    stops: chosen.stints.slice(1).map((st, k) => ({ boxLap: chosen.stints[k]!.to_lap, ...stintSet(u, st, ratio) })),
    why,
  };
}
