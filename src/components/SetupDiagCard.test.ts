import { describe, expect, it } from "vitest";
import { balanceRows, fmtSpread, fmtStat } from "./SetupDiagCard";
import type { BalanceZone } from "../lib/types";

const st = (value: number, laps = 30, spread = 1.4) => ({ value, unit: "°C", laps, spread });

describe("diagnostic setup", () => {
  it("affiche valeur, unité et dispersion en français", () => {
    expect(fmtStat(st(58.74))).toBe("58,7 °C");
    expect(fmtStat(null)).toBe("—");
    expect(fmtSpread(st(5, 30, 1.4))).toBe("±0,7 sur 30 tours");
  });
  it("range l'équilibre par virage puis entrée / milieu / sortie", () => {
    const z = (corner: string, phase: BalanceZone["phase"]): BalanceZone => ({ corner, phase, trend: "pas_d_ecart", ratio: { ...st(1), unit: "" }, countersteer_pct: 0 });
    const rows = balanceRows([z("Ascari", "sortie"), z("Rettifilo", "entree"), z("Ascari", "entree")]);
    expect(rows.map((r) => r.corner)).toEqual(["Ascari", "Rettifilo"]);
    expect(rows[0]!.cells.map((c) => c?.phase ?? null)).toEqual(["entree", null, "sortie"]);
  });
});
