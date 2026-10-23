import { describe, expect, it } from "vitest";
import { at, indexAtTime, positionsAt } from "./replay";
import type { Comparison } from "./types";

describe("relecture", () => {
  it("retrouve l'index fractionnaire d'un instant", () => {
    const t = [0, 0.5, 1.5, 2];
    expect(indexAtTime(t, -1)).toBe(0);
    expect(indexAtTime(t, 1)).toBeCloseTo(1.5);
    expect(indexAtTime(t, 9)).toBe(3);
  });
  it("interpole une valeur", () => {
    expect(at([10, 20, 40], 1.25)).toBeCloseTo(25);
    expect(at(null, 1)).toBeNull();
  });
  it("place la référence au même instant ou au même endroit", () => {
    const dist = [0, 10, 20, 30, 40];
    const c = { ref_trace: { dist, time: [0, 0.5, 1, 1.5, 2] }, cmp_trace: { dist, time: [0, 1, 2, 3, 4] } } as unknown as Comparison;
    // À 2 s, tu es à 20 m ; la référence, plus rapide, est déjà à 40 m.
    expect(positionsAt(c, 2)).toEqual({ ci: 2, ri: 4 });
    expect(positionsAt(c, 2, "distance")).toEqual({ ci: 2, ri: 2 });
  });
});
