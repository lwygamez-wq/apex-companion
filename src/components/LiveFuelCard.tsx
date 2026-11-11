import type { LiveFuel } from "../lib/types";
import { amount } from "./FuelCalculator";

const n1 = (v: number) => v.toFixed(1).replace(".", ",");
const n2 = (v: number) => v.toFixed(2).replace(".", ",");

/**
 * Relais en cours, mis à jour chaque seconde par le coach : niveau (énergie virtuelle si la voiture
 * en a, sinon carburant), consommation des 3 derniers tours, tours possibles et, en course, de quoi finir.
 */
export function LiveFuelCard({ f }: { f: LiveFuel | null | undefined }) {
  if (!f) return null;
  const u = f.unit;
  const pct = f.capacity ? Math.max(0, Math.min(1, f.level / f.capacity)) : null;
  let verdict: { cls: string; text: string } | null = null;
  if (f.laps_left != null && f.need_to_finish != null) {
    const spare = f.level - f.need_to_finish;
    if (spare >= 0) verdict = { cls: "ok", text: `De quoi finir : il faut ${amount(u, f.need_to_finish)}, marge ${amount(u, spare)}.` };
    else if (f.save_per_lap != null) verdict = { cls: "warn", text: `Il manque ${amount(u, -spare)} : économise ${u === "energy" ? `${n2(f.save_per_lap)} %` : `${n2(f.save_per_lap)} L`} par tour pour finir sans arrêt.` };
    else verdict = { cls: "warn", text: `Il manque ${amount(u, -spare)} : ${f.stops_needed ?? 1} arrêt${(f.stops_needed ?? 1) > 1 ? "s" : ""} nécessaire${(f.stops_needed ?? 1) > 1 ? "s" : ""}.` };
  }
  return (
    <div className="card livefuel">
      <div className="row" style={{ justifyContent: "space-between" }}>
        <h2 style={{ margin: 0 }}>Relais en cours</h2>
        <span className="muted" style={{ fontSize: 14 }}>{u === "energy" ? "énergie virtuelle" : "carburant"} · dis « carburant » pour l'entendre</span>
      </div>
      {pct != null && (
        <div className="lf-bar" aria-hidden="true">
          <i style={{ width: `${pct * 100}%` }} />
        </div>
      )}
      <div className="lf-main">
        <div><span>{u === "energy" ? "Énergie" : "Carburant"}</span><b className="mono">{amount(u, f.level)}</b>{f.fuel_l != null && <small className="muted">{n1(f.fuel_l)} L à bord</small>}</div>
        <div><span>Par tour</span><b className="mono">{f.per_lap != null ? (u === "energy" ? `${n2(f.per_lap)} %` : `${n2(f.per_lap)} L`) : "—"}</b><small className="muted">3 derniers tours</small></div>
        <div><span>Tours possibles</span><b className="mono">{f.laps_in_tank != null ? n1(f.laps_in_tank) : "—"}</b></div>
        <div><span>Tours restants</span><b className="mono">{f.laps_left != null ? n1(f.laps_left) : "—"}</b><small className="muted">{f.laps_left != null ? "course" : "hors course"}</small></div>
      </div>
      {f.per_lap == null && <div className="muted" style={{ marginTop: 10 }}>Consommation mesurée à la fin du premier tour lancé.</div>}
      {verdict && <div className={`lf-verdict ${verdict.cls}`}>{verdict.text}</div>}
    </div>
  );
}
