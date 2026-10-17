import { describe, expect, it } from "vitest";
import { at, indexAtTime } from "./replay";

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
});
