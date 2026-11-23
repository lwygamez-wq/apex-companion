import { describe, expect, it } from "vitest";
import { glanceTiles, liveState } from "./LiveGlance";
import type { EngineerCall, LiveFuel, StatusPayload } from "../lib/types";

const fuel = (e: Partial<EngineerCall> | null, extra: Partial<LiveFuel> = {}): LiveFuel => ({
  unit: "energy", level: 24.1, capacity: 100, fuel_l: 15.6, per_lap: 4.35, laps_in_tank: 5.4, laps_left: 18, need_to_finish: 78.3, stops_needed: 1, save_per_lap: null, fuel_ratio: 0.65, fuel_laps: 5.9,
  engineer: e ? ({ mode: "auto", stops_left: 1, stint: 1, stints: 2, target_per_lap: 4.38, per_lap: 4.35, verdict: "on_target", box_lap: null, laps_to_box: null, fill: null, fill_fuel_l: null, fuel_ratio: 0.65, fuel_limits: false, finish_spare_laps: null, ...e } as EngineerCall) : null,
  ...extra,
});

describe("Au volant, trois tuiles", () => {
  it("jamais plus de 3 informations, réserve et tours restants", () => {
    const t = glanceTiles(fuel({}), 18);
    expect(t).toHaveLength(3);
    expect(t.map((x) => x.value)).toEqual(["Dans la cible", "24,1 %", "18,0"]);
    expect(t[1]!.sub).toBe("5,4 tours possibles");
  });
  it("box : le tour de l'arrêt, puis « Box ce tour » en couleur d'action", () => {
    expect(glanceTiles(fuel({ box_lap: 21, laps_to_box: 3 }), 18)[0]).toMatchObject({ value: "Box fin T21", sub: "dans 3 tours", tone: "neutral" });
    expect(glanceTiles(fuel({ box_lap: 21, laps_to_box: 1 }), 20)[0]).toMatchObject({ value: "Box ce tour", tone: "accent" });
  });
  it("lift and coast en jaune (perte), sans ingénieur : de quoi finir ou arrêt nécessaire", () => {
    expect(glanceTiles(fuel({ verdict: "lift" }), 5)[0]!.tone).toBe("loss");
    expect(glanceTiles(fuel(null), 5)[0]).toMatchObject({ value: "Arrêt nécessaire", tone: "danger" });
    expect(glanceTiles(fuel(null, { level: 90 }), 5)[0]).toMatchObject({ value: "De quoi finir", tone: "gain" });
  });
  it("rien sans relais en cours", () => {
    expect(glanceTiles(null, null)).toEqual([]);
  });
  it("état de la source live en clair", () => {
    const s = (state: StatusPayload["state"], on_track = true) => ({ state, label: "LMU : en attente", live: { on_track } }) as unknown as StatusPayload;
    expect(liveState(s({ state: "session_active", sim: "lmu" }))).toEqual({ text: "En piste", tone: "gain" });
    expect(liveState(s({ state: "waiting_for_session", sim: "lmu" })).tone).toBe("loss");
    expect(liveState(null).text).toBe("Démarrage…");
  });
});
