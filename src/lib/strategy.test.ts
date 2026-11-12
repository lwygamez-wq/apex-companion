import { describe, expect, it } from "vitest";
import { consigne, convert, up } from "./strategy";
import type { PlanOption } from "./types";

const base = { ratio: 0.62, perLap: 4.82, marginLaps: 0.5, tankL: 90 };

describe("convertisseur énergie / ratio / litres", () => {
  it("énergie et ratio donnent les litres et les tours", () => {
    const r = convert({ ...base, from: "energy", value: 87 })!;
    expect(r.litres).toBeCloseTo(53.94, 2);
    expect(r.laps).toBeCloseTo(87 / 4.82 - 0.5, 4);
    expect(r.overFull || r.overTank).toBe(false);
  });
  it("les litres donnent l'énergie", () => {
    expect(convert({ ...base, from: "litres", value: 62 })!.energy).toBeCloseTo(100, 6);
  });
  it("les tours voulus donnent l'énergie à régler, marge comprise", () => {
    const r = convert({ ...base, from: "laps", value: 15 })!;
    expect(r.energy).toBeCloseTo(15.5 * 4.82, 6);
    expect(r.laps).toBeCloseTo(15, 6);
  });
  it("signale ce qui ne passe pas", () => {
    expect(convert({ ...base, from: "laps", value: 25 })!.overFull).toBe(true);
    // Ratio 1,0 et réservoir 90 L : 100 % demanderaient 100 L, on plafonne à 90 %.
    const r = convert({ ...base, ratio: 1, from: "energy", value: 100 })!;
    expect(r.overTank).toBe(true);
    expect(r.maxEnergy).toBeCloseTo(90, 6);
    expect(convert({ ...base, perLap: null, from: "laps", value: 10 })).toBeNull();
    expect(convert({ ...base, ratio: 0, from: "energy", value: 50 })).toBeNull();
  });
  it("arrondit au-dessus sans ajouter 1 par erreur", () => {
    expect(up(100.00001)).toBe(100);
    expect(up(86.2)).toBe(87);
  });
});

const st = (from: number, to: number, amount: number) => ({ from_lap: from, to_lap: to, amount, fuel_l: null, duration_s: 0 });
const attack: PlanOption = { kind: "attack", stops: 5, target_per_lap: 5.42, saving: null, window: null, stints: [st(1, 19, 100), st(20, 35, 86), st(36, 52, 91), st(53, 69, 91), st(70, 86, 91), st(87, 104, 91)] };
const save: PlanOption = { kind: "save", stops: 4, target_per_lap: 4.65, saving: 0.036, window: null, stints: [st(1, 20, 100), st(21, 41, 100), st(42, 62, 100), st(63, 83, 100), st(84, 104, 100)] };

describe("consigne", () => {
  it("plan économie : quoi mettre, barre à tenir, arrêts", () => {
    const c = consigne("energy", save, [attack, save], 4.82, 0.62)!;
    expect(c.start).toEqual({ set: 100, litres: 62 });
    expect(c.bar).toBe(4.65);
    expect(c.mustSave).toBe(true);
    expect(c.stops.map((s) => s.boxLap)).toEqual([20, 41, 62, 83]);
    expect(c.stops[0]).toEqual({ boxLap: 20, set: 100, litres: 62 });
    expect(c.why).toContain("4 arrêts au lieu de 5");
  });
  it("plan attaque : pas d'économie, niveaux ajustés, et ce que coûterait un arrêt de moins", () => {
    const c = consigne("energy", attack, [attack, save], 4.82, 0.62)!;
    expect(c.mustSave).toBe(false);
    expect(c.stops[0]).toEqual({ boxLap: 19, set: 86, litres: 53 });
    expect(c.why).toContain("4,65 %");
  });
  it("litres : pas de ratio", () => {
    const c = consigne("litres", { ...attack, stints: [st(1, 20, 46.4)] }, [attack], 1.6, null)!;
    expect(c.start).toEqual({ set: 47, litres: null });
    expect(c.stops).toEqual([]);
  });
});
