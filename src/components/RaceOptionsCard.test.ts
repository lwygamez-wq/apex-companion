import { describe, expect, it } from "vitest";
import { roleOf, verdictText } from "./RaceOptionsCard";
import type { RaceCandidate, RaceOptions } from "../lib/types";

const c = (stops: number, pit: number): RaceCandidate => ({ stops, saving: 0, target_per_lap: 4.35, laps: 36, window: [14, 22], fills: [62.4], pit_time_s: pit, pit_time_sd_s: 2, arrival_laps: 0.5, arrival_sd_laps: 0.1, breakeven_s_per_lap: null, wear_left_pct: 85 });
const base = (verdict: RaceOptions["verdict"]): RaceOptions => ({ unit: "energy", candidates: [c(1, 53.5), c(2, 83.5)], verdict, missing: [], radio: "", pit_model: null });

describe("RaceOptionsCard", () => {
  it("annonce la plus rapide avec l'écart et sa fourchette (deux écarts types)", () => {
    const o = base({ kind: "clear", best: 0, second: 1, delta_s: 30, sd_s: 2 });
    expect(verdictText(o)).toBe("1 arrêt : 30 s de moins au stand que 2 arrêts (± 4 s).");
    expect(roleOf(o, 0)).toBe("la plus rapide");
    expect(roleOf(o, 1)).toBeNull();
  });
  it("écart dans l'incertitude : équivalentes, la plus sûre désignée", () => {
    const o = base({ kind: "equivalent", safest: 1, others: [0], delta_s: 3, sd_s: 2 });
    expect(verdictText(o)).toContain("options équivalentes, 2 arrêts est la plus sûre");
    expect(roleOf(o, 0)).toBe("équivalente");
  });
  it("sans mesure : pas de classement", () => {
    expect(verdictText(base({ kind: "unknown", reason: "temps d'arrêt non mesuré" }))).toBe("Pas de comparaison : temps d'arrêt non mesuré.");
  });
});
