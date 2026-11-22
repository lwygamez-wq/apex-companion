import { describe, expect, it } from "vitest";
import { gainOverPrevious } from "./ReferencesPage";
import type { ReferenceEntry } from "../lib/types";

const e = (t: number, prev: number[]): ReferenceEntry => ({ lap_time_s: t, history: prev.map((p) => ({ lap_time_s: p })) }) as unknown as ReferenceEntry;

describe("gainOverPrevious", () => {
  it("compare à la version remplacée la plus récente", () => {
    expect(gainOverPrevious(e(92.921, [93.689, 94.2]))).toBeCloseTo(0.768, 6);
  });
  it("première référence : pas de gain", () => {
    expect(gainOverPrevious(e(92.921, []))).toBeNull();
  });
});
