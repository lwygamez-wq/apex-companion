import { describe, expect, it } from "vitest";
import { amount, parseLapTime } from "../components/FuelCalculator";

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
  });
});
