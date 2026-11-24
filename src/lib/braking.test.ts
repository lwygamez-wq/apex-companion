import { describe, expect, it } from "vitest";
import { brakeGapText, lossText, minSpeedText, priorityCorners, throttleGapText } from "./braking";
import type { BrakeCompare, Comparison, CornerComparison } from "./types";

const b = (o: Partial<BrakeCompare>): BrakeCompare => ({ reference: null, compared: null, brake_gap_m: null, min_speed_gap_kmh: 0, throttle_gap_m: null, missing: null, ...o });

describe("textes de freinage", () => {
  it("donne l'écart de point de freinage avec son unité, jamais sans mesure", () => {
    expect(brakeGapText(b({ brake_gap_m: -18 }))).toBe("18 m trop tôt");
    expect(brakeGapText(b({ brake_gap_m: 6 }))).toBe("6 m plus tard");
    expect(brakeGapText(b({ brake_gap_m: 3 }))).toBe("au même endroit");
    expect(brakeGapText(b({ brake_gap_m: null, missing: "ton tour : canal frein absent" }))).toBeNull();
  });
  it("vitesse mini et gaz", () => {
    expect(minSpeedText(b({ min_speed_gap_kmh: -4.2 }))).toBe("−4 km/h à la corde");
    expect(minSpeedText(b({ min_speed_gap_kmh: 0.4 }))).toBeNull();
    expect(throttleGapText(b({ throttle_gap_m: 12 }))).toBe("gaz 12 m plus tard");
    expect(throttleGapText(b({ throttle_gap_m: -3 }))).toBeNull();
    expect(lossText(0.318)).toBe("+0,32 s");
  });
  it("retient les 3 virages qui coûtent le plus, au-delà de 0,05 s", () => {
    const k = (id: number, dt: number) => ({ corner: { id }, time_delta_s: dt }) as unknown as CornerComparison;
    const c = { corners: [k(1, 0.1), k(2, 0.4), k(3, -0.2), k(4, 0.04), k(5, 0.2), k(6, 0.3)] } as unknown as Comparison;
    expect(priorityCorners(c).map((x) => x.corner.id)).toEqual([2, 6, 5]);
  });
});
