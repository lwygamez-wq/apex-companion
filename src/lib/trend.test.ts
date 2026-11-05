import { describe, expect, it } from "vitest";
import { direction, sessionTrend } from "./trend";
import type { HistoryLap } from "./types";

const lap = (session: number, t: number | null, clean: boolean, kind: HistoryLap["kind"] = "flying"): HistoryLap => ({
  lap_id: Math.random(), session_id: session, session_kind: "practice", started_at_unix: session * 1000, number: 1, kind,
  lap_time_s: t, clean, off_track: !clean, incident: false, is_reference: false, delta_to_best_s: null, personal_best_at_time: false,
});

describe("sessionTrend", () => {
  it("compte les tours lancés seulement et calcule part propre, dispersion et meilleur", () => {
    const pts = sessionTrend([lap(2, 61, true), lap(2, 60, true), lap(2, 62, true), lap(2, 70, false), lap(2, 90, false, "out_lap"), lap(1, 65, false)]);
    expect(pts.map((p) => p.session_id)).toEqual([1, 2]);
    expect(pts[0]).toMatchObject({ timed: 1, clean: 0, clean_rate: 0, best_s: null, spread_s: null });
    expect(pts[1]).toMatchObject({ timed: 4, clean: 3, clean_rate: 0.75, best_s: 60 });
    expect(pts[1]!.spread_s).toBeCloseTo(1, 5);
  });
  it("donne le sens de la tendance", () => {
    expect(direction([0.5, 0.5, 0.5, 0.8, 0.8, 0.8], true)).toBe("better");
    expect(direction([0.5, 0.5, 0.5, 0.8, 0.8, 0.8], false)).toBe("worse");
    expect(direction([1, 1, 1], true)).toBeNull();
  });
});

import { bilan } from "../components/NewSessionBanner";
import type { SessionSummary } from "./types";
const ss = (id: number, best: number | null, at: number): SessionSummary =>
  ({ id, sim: "iracing", sim_name: "iRacing", track: "Road Atlanta", track_config: null, car: "SF23", kind: "practice", started_at_unix: at, lap_count: 10, clean_lap_count: 6, best_lap_s: best } as unknown as SessionSummary);
describe("bilan", () => {
  it("compare au record d'avant sur le combo", () => {
    expect(bilan(ss(3, 64.2, 30), [ss(1, 64.5, 10), ss(2, 64.4, 20), ss(3, 64.2, 30)])).toContain("nouveau record 1:04.200");
    expect(bilan(ss(3, 64.6, 30), [ss(1, 64.5, 10), ss(3, 64.6, 30)])).toContain("du record");
    expect(bilan(ss(1, 64.6, 10), [ss(1, 64.6, 10)])).toContain("premier record");
  });
});
