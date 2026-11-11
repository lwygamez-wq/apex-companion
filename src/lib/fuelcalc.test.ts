import { describe, expect, it } from "vitest";
import { amount, parseLapTime, perLap, planFor, setting } from "../components/FuelCalculator";
import type { PlanOption } from "./types";

const opt = (kind: PlanOption["kind"], stops: number, saving: number | null): PlanOption => ({ kind, stops, target_per_lap: 4.8, saving, stints: [], window: null });

describe("calculateur carburant", () => {
  it("lit un temps au tour saisi à la main", () => {
    expect(parseLapTime("3:31.4")).toBeCloseTo(211.4, 6);
    expect(parseLapTime("3:31,4")).toBeCloseTo(211.4, 6);
    expect(parseLapTime("211.4")).toBeCloseTo(211.4, 6);
    expect(parseLapTime("1:05")).toBe(65);
    expect(parseLapTime("")).toBeNull();
    expect(parseLapTime("abc")).toBeNull();
  });
  it("affiche litres et énergie", () => {
    expect(amount("litres", 46)).toBe("46,0 L");
    expect(amount("energy", 96.4)).toBe("96,4 %");
    expect(perLap("energy", 4.647)).toBe("4,65 %");
    // Niveau à régler arrondi au-dessus, sans que le bruit de calcul ajoute 1.
    expect(setting("energy", 96.41)).toBe("97 %");
    expect(setting("energy", 100.00001)).toBe("100 %");
    expect(setting("litres", 45.2)).toBe("46 L");
  });
  it("affiche le plan que l'ingénieur suivra", () => {
    const light = [opt("attack", 5, null), opt("save", 4, 0.036)];
    const heavy = [opt("attack", 5, null), opt("save", 4, 0.07)];
    expect(planFor(light, "auto")?.stops).toBe(4);
    expect(planFor(heavy, "auto")?.stops).toBe(5);
    expect(planFor(heavy, "save")?.stops).toBe(4);
    expect(planFor(light, "attack")?.stops).toBe(5);
    expect(planFor([opt("attack", 2, null)], "save")?.stops).toBe(2);
    expect(planFor([], "auto")).toBeNull();
  });
});
