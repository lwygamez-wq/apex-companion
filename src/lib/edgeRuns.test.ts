import { describe, expect, it } from "vitest";
import { edgeRuns } from "../components/TrackMap";

describe("edgeRuns", () => {
  const edge: [number, number][] = Array.from({ length: 20 }, (_, i) => [i * 4, 0]);
  it("trace un tronçon continu le long du bord, trous d'un point comblés, bruit isolé ignoré", () => {
    const marks: [number, number][] = [[17, 1], [21, 1], [30, 1], [33, -1], [64, 9]];
    const d = edgeRuns(edge, marks);
    expect(d.startsWith("M16.0 0.0")).toBe(true);
    expect(d).toContain("L36.0 0.0");
    expect(d).toContain("L24.0 0.0"); // trou d'un point comblé
    expect(d.match(/M/g)?.length).toBe(1); // le relevé à 9 m du bord ne fait pas un vibreur
  });
  it("rien sans relevé", () => {
    expect(edgeRuns(edge, [])).toBe("");
  });
});
