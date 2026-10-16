import { describe, expect, it } from "vitest";
import { decimate, delta, lapTime } from "./format";

describe("lapTime", () => {
  it("formate minutes et millièmes", () => {
    expect(lapTime(83.456)).toBe("1:23.456");
    expect(lapTime(60)).toBe("1:00.000");
    expect(lapTime(59.9996)).toBe("1:00.000");
    expect(lapTime(9.5)).toBe("9.500");
  });
  it("n'invente rien pour une valeur absente", () => {
    expect(lapTime(null)).toBe("—");
    expect(lapTime(Number.NaN)).toBe("—");
  });
});

describe("delta", () => {
  it("signe explicitement", () => {
    expect(delta(0.182)).toBe("+0.182");
    expect(delta(-0.05)).toBe("−0.050");
    expect(delta(-0.0001)).toBe("±0.000");
  });
});

describe("decimate", () => {
  it("garde le premier et le dernier point", () => {
    expect(decimate([0, 1, 2, 3, 4, 5, 6], 3)).toEqual([0, 3, 6]);
    expect(decimate([0, 1, 2, 3, 4], 3)).toEqual([0, 3, 4]);
  });
});
