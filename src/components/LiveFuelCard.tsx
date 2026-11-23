import type { EngineerCall, FuelUnit, LiveFuel } from "../lib/types";
import { perLap, setting } from "./FuelCalculator";

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
          {u === "energy" && e.mfd_energy != null && (
            <span className={`lf-mfd ${Math.abs(e.mfd_energy - Math.ceil(e.fill - 0.05)) < 1.5 ? "ok" : "warn"}`}>
              {" "}· MFD du jeu <b className="mono">{Math.round(e.mfd_energy)} %</b>
              {Math.abs(e.mfd_energy - Math.ceil(e.fill - 0.05)) < 1.5 ? " (bon)" : " (à changer)"}
            </span>
          )}
        </div>
      )}
      {e.game_laps_for_fill != null && e.laps_after_stop != null && (
        <div className={`lf-verdict ${e.game_laps_for_fill + 0.5 < e.laps_after_stop ? "warn" : ""}`}>
          Le jeu compte {n1(e.game_laps_for_fill)} tours avec ce niveau, il en faut {e.laps_after_stop}.
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
  return (
    <div className="card livefuel">
      <div className="row row-between">
        <h2 className="m-0">Détail du relais</h2>
        <span className="muted t-sm">{u === "energy" ? "énergie virtuelle" : "carburant"} · dis « stratégie » ou « {u === "energy" ? "énergie" : "carburant"} »</span>
      </div>
      {pct != null && (
        <div className="lf-bar" aria-hidden="true">
          <i style={{ width: `${pct * 100}%` }} />
        </div>
      )}
      <div className="lf-main">
        <div><span>Par tour</span><b className="mono">{f.per_lap != null ? perLap(u, f.per_lap) : "—"}</b><small className="muted">3 derniers tours</small></div>
        {f.fuel_l != null && <div><span>Carburant à bord</span><b className="mono">{n1(f.fuel_l)} L</b>{f.fuel_laps != null && <small className="muted">{n1(f.fuel_laps)} tours</small>}</div>}
        {u === "energy" && f.fuel_ratio != null && <div><span>Ratio conseillé</span><b className="mono">{n2(f.fuel_ratio)}</b><small className="muted">L par %</small></div>}
      </div>
      {f.per_lap == null && <div className="muted mt-12">Consommation mesurée à la fin du premier tour lancé.</div>}
      {f.engineer && <EngineerBlock e={f.engineer} u={u} />}
    </div>
  );
}
