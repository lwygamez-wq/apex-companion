import type { EngineerCall, FuelUnit, LiveFuel } from "../lib/types";
import { amount, perLap, setting } from "./FuelCalculator";

const n1 = (v: number) => v.toFixed(1).replace(".", ",");
const n2 = (v: number) => v.toFixed(2).replace(".", ",");

const VERDICT: Record<EngineerCall["verdict"], string> = {
  lift: "Lift and coast",
  on_target: "Dans la cible",
  push: "Attaque",
};
const MODE: Record<EngineerCall["mode"], string> = { auto: "plan auto", attack: "plan attaque", save: "plan économie" };

/** Consigne de l'ingénieur de course : cible, verdict, arrêt et réglages au stand. */
function EngineerBlock({ e, u }: { e: EngineerCall; u: FuelUnit }) {
  const gap = e.per_lap - e.target_per_lap;
  return (
    <div className={`lf-eng ${e.verdict}`}>
      <div className="lf-eng-top">
        <span className="muted">Ingénieur · {MODE[e.mode]} · relais {e.stint} sur {e.stints}</span>
        <span className={`lf-pill ${e.verdict}`}>{VERDICT[e.verdict]}</span>
      </div>
      <div className="lf-eng-main">
        <div><span>Cible</span><b className="mono">{perLap(u, e.target_per_lap)}</b><small className="muted">par tour</small></div>
        <div><span>Tu fais</span><b className="mono">{perLap(u, e.per_lap)}</b><small className="muted">{e.verdict === "lift" ? `${perLap(u, gap)} de trop` : e.verdict === "push" ? `${perLap(u, -gap)} de marge` : "3 derniers tours"}</small></div>
        <div>
          <span>{e.box_lap != null ? "Box" : "Arrivée"}</span>
          <b className="mono">{e.box_lap != null ? `fin T${e.box_lap}` : `${e.laps_left} t.`}</b>
          <small className="muted">{e.laps_to_box != null ? `dans ${e.laps_to_box} tour${e.laps_to_box > 1 ? "s" : ""}` : e.finish_spare_laps != null ? `marge ${n1(Math.max(0, e.finish_spare_laps))} tour` : ""}</small>
        </div>
      </div>
      {e.fill != null && (
        <div className="lf-pit">
          Au stand : <b className="mono">{setting(u, e.fill)}</b>
          {u === "energy" && e.fuel_ratio != null && <> · ratio <b className="mono">{n2(e.fuel_ratio)}</b>{e.fill_fuel_l != null && <span className="muted"> ({Math.round(e.fill_fuel_l)} L)</span>}</>}
        </div>
      )}
      {e.fuel_limits && <div className="lf-verdict warn">Le carburant lâchera avant l'énergie : ratio trop bas. Au prochain arrêt, ratio {e.fuel_ratio != null ? n2(e.fuel_ratio) : "plus haut"}.</div>}
    </div>
  );
}

/**
 * Relais en cours, mis à jour chaque seconde par le coach : niveau (énergie virtuelle si la voiture
 * en a, sinon carburant), consommation des 3 derniers tours, tours possibles et, en course, la
 * consigne de l'ingénieur (cible, lift ou attaque, arrêt, énergie et ratio à régler).
 */
export function LiveFuelCard({ f }: { f: LiveFuel | null | undefined }) {
  if (!f) return null;
  const u = f.unit;
  const pct = f.capacity ? Math.max(0, Math.min(1, f.level / f.capacity)) : null;
  let verdict: { cls: string; text: string } | null = null;
  if (!f.engineer && f.laps_left != null && f.need_to_finish != null) {
    const spare = f.level - f.need_to_finish;
    if (spare >= 0) verdict = { cls: "ok", text: `De quoi finir : il faut ${amount(u, f.need_to_finish)}, marge ${amount(u, spare)}.` };
    else if (f.save_per_lap != null) verdict = { cls: "warn", text: `Il manque ${amount(u, -spare)} : économise ${perLap(u, f.save_per_lap)} par tour pour finir sans arrêt.` };
    else verdict = { cls: "warn", text: `Il manque ${amount(u, -spare)} : ${f.stops_needed ?? 1} arrêt${(f.stops_needed ?? 1) > 1 ? "s" : ""} nécessaire${(f.stops_needed ?? 1) > 1 ? "s" : ""}.` };
  }
  return (
    <div className="card livefuel">
      <div className="row" style={{ justifyContent: "space-between" }}>
        <h2 style={{ margin: 0 }}>Relais en cours</h2>
        <span className="muted" style={{ fontSize: 14 }}>{u === "energy" ? "énergie virtuelle" : "carburant"} · dis « stratégie » ou « {u === "energy" ? "énergie" : "carburant"} »</span>
      </div>
      {pct != null && (
        <div className="lf-bar" aria-hidden="true">
          <i style={{ width: `${pct * 100}%` }} />
        </div>
      )}
      <div className="lf-main">
        <div><span>{u === "energy" ? "Énergie" : "Carburant"}</span><b className="mono">{amount(u, f.level)}</b>{f.fuel_l != null && <small className="muted">{n1(f.fuel_l)} L à bord{f.fuel_laps != null ? ` · ${n1(f.fuel_laps)} tours` : ""}</small>}</div>
        <div><span>Par tour</span><b className="mono">{f.per_lap != null ? perLap(u, f.per_lap) : "—"}</b><small className="muted">3 derniers tours</small></div>
        <div><span>Tours possibles</span><b className="mono">{f.laps_in_tank != null ? n1(f.laps_in_tank) : "—"}</b>{u === "energy" && f.fuel_ratio != null && <small className="muted">ratio conseillé {n2(f.fuel_ratio)}</small>}</div>
        <div><span>Tours restants</span><b className="mono">{f.laps_left != null ? n1(f.laps_left) : "—"}</b><small className="muted">{f.laps_left != null ? "course" : "hors course"}</small></div>
      </div>
      {f.per_lap == null && <div className="muted" style={{ marginTop: 10 }}>Consommation mesurée à la fin du premier tour lancé.</div>}
      {f.engineer && <EngineerBlock e={f.engineer} u={u} />}
      {verdict && <div className={`lf-verdict ${verdict.cls}`}>{verdict.text}</div>}
    </div>
  );
}
