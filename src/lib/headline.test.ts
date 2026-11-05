import { describe, expect, it } from "vitest";
import { headline } from "./headline";
import type { Comparison } from "./types";

const cmp = (deltas: [string, number][]): Comparison =>
  ({ compared: { number: 16, lap_id: 2 }, reference: { lap_id: 1, driver: null, number: 12 }, corners: deltas.map(([name, d]) => ({ corner: { name }, time_delta_s: d })) } as unknown as Comparison);
const txt = (h: ReturnType<typeof headline>) => `${h.lead}${h.amount ?? ""}${h.mid}${h.where ?? ""}${h.tail}`;

describe("headline", () => {
  it("nomme les deux virages les plus coûteux", () => {
    expect(txt(headline(cmp([["Virage 1", 0.42], ["V3", 0.13], ["Virage 6", 0.44]]), 1.717))).toBe("Ton tour 16 a perdu 1,7 s sur ta référence, surtout aux virages 6 et 1.");
  });
  it("un seul virage, et le gain", () => {
    expect(txt(headline(cmp([["Virage 1", -0.3]]), -0.4))).toBe("Ton tour 16 a gagné 0,4 s sur ta référence, surtout au virage 1.");
  });
  it("noms libres et écart nul", () => {
    expect(txt(headline(cmp([["Chicane", 0.3], ["Épingle", 0.2]]), 0.6))).toBe("Ton tour 16 a perdu 0,6 s sur ta référence, surtout à Chicane et Épingle.");
    expect(txt(headline(cmp([]), 0.01))).toBe("Ton tour 16 est au niveau de ta référence.");
  });
});
