import type { CircuitOverview, LapRow } from "../lib/types";
import { FuelCalculator } from "./FuelCalculator";

const n2 = (v: number) => v.toFixed(2).replace(".", ",");
const avg = (v: number[]) => (v.length ? v.reduce((a, b) => a + b, 0) / v.length : null);
const WHEELS = ["AvG", "AvD", "ArG", "ArD"];

/**
 * Consommation et pneus de la séance (tours lancés propres) : énergie virtuelle et carburant par tour,
 * usure de chaque pneu par tour ; et le calculateur de relais pour ce circuit.
 */
export function ConsumptionCard({ laps, combo }: { laps: LapRow[]; combo: Pick<CircuitOverview, "sim" | "track" | "track_config" | "car" | "setup"> }) {
  const ok = laps.filter((l) => l.clean && l.kind === "flying");
  const energy = avg(ok.map((l) => l.energy_used_pct ?? null).filter((v): v is number => v != null && v > 0.1));
  const fuel = avg(ok.map((l) => l.fuel_used_l).filter((v): v is number => v != null && v > 0.05));
  const wears = ok.map((l) => l.tyre_wear_pct ?? null).filter((w): w is [number, number, number, number] => w != null);
  const wear = wears.length ? [0, 1, 2, 3].map((k) => avg(wears.map((w) => w[k] ?? 0)) ?? 0) : null;
  if (energy == null && fuel == null && !wear) return null;
  return (
    <div className="card consumption" style={{ marginTop: 14 }}>
      <h2>Consommation et pneus</h2>
      <div className="strategy-facts">
        {energy != null && <div><span>Énergie par tour</span><b className="mono">{n2(energy)} %</b><small className="muted">100 % ≈ {Math.floor(100 / energy)} tours</small></div>}
        {fuel != null && <div><span>Carburant par tour</span><b className="mono">{n2(fuel)} L</b></div>}
        {wear && wear.map((w, k) => (
          <div key={k} className={w === Math.max(...wear) ? "worst" : ""}><span>Pneu {WHEELS[k]}</span><b className="mono">−{n2(w)} %</b><small className="muted">par tour</small></div>
        ))}
      </div>
      <div className="muted" style={{ fontSize: 13, marginTop: 8 }}>Moyenne de tes {ok.length} tours propres de la séance.</div>
      <details className="consumption-calc">
        <summary>Calculer une course ici (arrêts, niveau à reprendre, pneus)</summary>
        <FuelCalculator combo={combo} title="Calculateur carburant / énergie" />
      </details>
    </div>
  );
}
