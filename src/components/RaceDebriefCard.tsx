import { lapTime } from "../lib/format";
import type { RaceDebrief } from "../lib/types";

const n1 = (v: number) => v.toFixed(1).replace(".", ",");
const KIND_LABEL: Record<RaceDebrief["losses"][number]["kind"], string> = { errors: "Tours ratés", stops: "Arrêts", overfill: "Plein", pace: "Rythme" };

/**
 * Débrief de course : où est parti le temps (pertes mesurées contre ta médiane de course), puis au
 * plus trois priorités. Chaque ligne vient d'une mesure ; ce qui n'a pas pu être mesuré est dit.
 */
export function RaceDebriefCard({ d }: { d: RaceDebrief }) {
  const max = Math.max(1, ...d.losses.map((l) => l.seconds));
  return (
    <div className="card race-debrief">
      <div className="rd-head">
        <h2 style={{ margin: 0 }}>Débrief de course</h2>
        <span className="muted">
          référence : ta médiane de course, {lapTime(d.median_s)} sur {d.clean_laps} tours sans incident
        </span>
      </div>

      <h3 className="rd-title">Tes priorités pour la prochaine course</h3>
      {d.actions.length === 0 ? (
        <p className="muted" style={{ margin: 0 }}>Aucune perte mesurable à corriger sur cette course.</p>
      ) : (
        <ol className="rd-actions">
          {d.actions.map((a) => (
            <li key={a.title}>
              <b>{a.title}</b>
              <span>{a.detail}</span>
            </li>
          ))}
        </ol>
      )}

      {d.losses.length > 0 && (
        <>
          <h3 className="rd-title">Où est parti le temps</h3>
          <div className="rd-losses">
            {d.losses.map((l) => (
              <div key={l.kind} className={`rd-loss ${l.kind}`}>
                <div className="rd-loss-top">
                  <span className="rd-kind">{KIND_LABEL[l.kind]}</span>
                  <b>{l.kind === "overfill" ? "≤ " : ""}{n1(l.seconds)} s</b>
                </div>
                <div className="rd-bar"><i style={{ width: `${Math.max(2, (l.seconds / max) * 100)}%` }} /></div>
                <p>{l.detail}</p>
              </div>
            ))}
          </div>
        </>
      )}

      {d.energy_per_lap != null && d.energy_finish != null && (
        <p className="rd-energy">
          Énergie : <b>{n1(d.energy_per_lap)} %</b> par tour · arrivée avec <b>{n1(d.energy_finish)} %</b>
          {d.spare_laps != null ? ` (${n1(d.spare_laps)} tours)` : ""}
          {d.fill_set != null && d.fill_needed != null ? ` · dernier plein réglé ${Math.round(d.fill_set)} %, nécessaire ${Math.round(d.fill_needed)} %` : ""}
        </p>
      )}

      {d.stints.length > 1 && (
        <p className="rd-energy">
          Rythme par relais :{" "}
          {d.stints.map((s, i) => `${i ? " · " : ""}tours ${s.from_lap}–${s.to_lap} ${lapTime(s.median_s)} (${lapTime(s.first_half_s)} puis ${lapTime(s.second_half_s)})`).join("")}
        </p>
      )}

      {d.missing.map((m) => (
        <p key={m} className="muted rd-missing">Non mesuré. {m}</p>
      ))}
    </div>
  );
}
