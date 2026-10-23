import { describe, expect, it } from "vitest";
import { buildView, cornerFacts, finalGap, fmtGap, gapMeters, interp, shortCorner } from "./compareView";
import type { Comparison, CornerComparison } from "./types";

/** Deux tours à vitesse constante sur 1000 m : référence 50 m/s (20 s), toi 40 m/s (25 s). */
function lap(v: number) {
  const dist = Array.from({ length: 1001 }, (_, i) => i);
  return { step_m: 1, dist, time: dist.map((d) => d / v), speed_kmh: dist.map(() => v * 3.6), throttle: dist.map(() => 1), brake: dist.map(() => 0), gear: null, steer_deg: null, x: null, y: null, coverage: 1, source_density: 1 };
}
const corner = { id: 1, name: "Virage 12", search_start_m: 300, entry_m: 400, apex_m: 500, exit_m: 600 };
const cmp = {
  step_m: 1,
  length_m: 1000,
  reference: { lap_id: 1, number: 1, lap_time_s: 20, clean: true, coverage: 1 },
  compared: { lap_id: 2, number: 2, lap_time_s: 25, clean: true, coverage: 1 },
  ref_trace: lap(50),
  cmp_trace: lap(40),
  delta_s: Array.from({ length: 1001 }, (_, d) => d / 40 - d / 50),
  line_offset_m: null,
  ref_off_track_m: [],
  cmp_off_track_m: [],
  corners: [],
  insights: [],
  warnings: [],
} as unknown as Comparison;
(cmp as { corners: unknown[] }).corners = [{ corner, time_delta_s: 0.1, apex_offset_m: null, reference: {}, compared: {} }];

describe("compareView", () => {
  it("interpole et refuse hors bornes", () => {
    expect(interp([0, 10], [0, 100], 5)).toBe(50);
    expect(interp([0, 10], [0, 100], 11)).toBeNull();
    expect(interp([0, 10], [0, 100], 10)).toBe(100);
  });
  it("écart en mètres : la référence est devant", () => {
    const g = gapMeters(cmp);
    // À 400 m (10 s pour toi), la référence est à 500 m.
    expect(g[400]).toBeCloseTo(100, 6);
    // Ligne franchie à 25 s : la référence, arrivée à 20 s, a roulé 5 s de plus à 50 m/s.
    expect(g[1000]).toBeCloseTo(250, 6);
    expect(finalGap(cmp, "m")).toBeCloseTo(250, 6);
    expect(finalGap(cmp, "s")).toBeCloseTo(5, 6);
  });
  it("axe temps : chaque tour à son chrono, virages placés à ton passage", () => {
    const v = buildView(cmp, "time", "s", 1, 0.5);
    expect(v.x[0]).toBe(0);
    expect(v.x[v.x.length - 1]).toBe(25);
    // Après 20 s, la référence est arrivée : plus de courbe.
    expect(v.ref.speed[v.x.indexOf(21)]).toBeNull();
    expect(v.cmp.speed[v.x.indexOf(21)]).toBeCloseTo(144, 6);
    expect(v.bands[0]).toMatchObject({ label: "V12", apex: 12.5 });
    // Écart en secondes au temps 10 s (toi à 400 m) : 400/40 − 400/50 = 2 s.
    expect(v.gap[v.x.indexOf(10)]).toBeCloseTo(2, 6);
  });
  it("axe distance : décimation et dernier point gardés", () => {
    const v = buildView(cmp, "distance", "m", 3);
    expect(v.x[v.x.length - 1]).toBe(1000);
    expect(v.bands[0]!.apex).toBe(500);
  });
  it("formats et noms courts", () => {
    expect(fmtGap(0.384, "s", 2)).toBe("+0.38 s");
    expect(fmtGap(-12.4, "m")).toBe("−12 m");
    expect(shortCorner("Virages 14-15")).toBe("V14-15");
    expect(shortCorner("Descida do Lago")).toBe("Descida do Lago");
  });
  it("faits marquants d'un virage, du plus fort au plus faible", () => {
    const m = (o: object) => ({ corner_id: 1, time_s: 0, brake_on_m: 300, brake_peak: 0.9, min_speed_kmh: 99, min_speed_at_m: 0, throttle_on_m: 407, exit_speed_kmh: 154, throttle_ramp_s: null, steer_reversals: null, ...o });
    const k = { corner, time_delta_s: 0.38, apex_offset_m: null, reference: m({}), compared: m({ brake_on_m: 280, min_speed_kmh: 88, throttle_on_m: 401, exit_speed_kmh: 153 }) } as CornerComparison;
    const f = cornerFacts(k);
    expect(f.map((x) => x.text)).toEqual(["11 km/h de moins à la corde", "freinage 20 m plus tôt"]);
    expect(f.every((x) => x.bad)).toBe(true);
  });
});
