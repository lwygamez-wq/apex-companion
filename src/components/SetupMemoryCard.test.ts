import { describe, expect, it } from "vitest";
import { queueLine } from "./SetupMemoryCard";
import type { QueueItem, Trial } from "../lib/types";

const trial = { label: "Carrossage AR", direction: "vers 0°", steps: 1 } as unknown as Trial;

describe("queueLine", () => {
  it("dit le réglage, le sens, les crans et la répétition du constat", () => {
    const q: QueueItem = { trial, sharpness: 5, repeats: 2, sessions_measured: 2, tried: null };
    expect(queueLine(q)).toBe("Carrossage AR vers 0°, 1 cran (constat sur 2 séances sur 2)");
  });
  it("signale un essai déjà fait sans effet", () => {
    const q: QueueItem = { trial, sharpness: 5, repeats: 1, sessions_measured: 3, tried: "déjà essayé" };
    expect(queueLine(q)).toBe("Carrossage AR vers 0°, 1 cran (constat sur 1 séance sur 3), déjà essayé sans effet");
  });
});
