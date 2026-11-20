import { lapTime } from "../lib/format";
import type { RaceDebrief } from "../lib/types";

const n1 = (v: number) => v.toFixed(1).replace(".", ",");
const KIND_LABEL: Record<RaceDebrief["losses"][number]["kind"], string> = { errors: "Tours ratés", stops: "Arrêts", overfill: "Plein", pace: "Rythme", class_pace: "Rythme catégorie" };
const signed = (v: number) => `${v > 0 ? "+" : v < 0 ? "−" : ""}${n1(Math.abs(v))} s`;

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

      {d.class_pace && (
        <div className="rd-class">
          <h3 className="rd-title">Face à ta catégorie</h3>
          <div className="rd-class-grid">
            <div><span>Écart par tour</span><b className={`mono ${d.class_pace.gap_s > 0.05 ? "slow" : "fast"}`}>{signed(d.class_pace.gap_s)}</b><small className="muted">contre la médiane</small></div>
            <div><span>Médiane catégorie</span><b className="mono">{lapTime(d.class_pace.class_median_s)}</b><small className="muted">jusqu'à {d.class_pace.cars} voitures</small></div>
            <div><span>Meilleur vu</span><b className="mono">{d.class_pace.class_best_s < 1e8 ? lapTime(d.class_pace.class_best_s) : "—"}</b><small className="muted">dans la catégorie</small></div>
            <div><span>Tours plus rapides</span><b className="mono">{d.class_pace.faster_laps} / {d.class_pace.laps}</b><small className="muted">que la médiane</small></div>
          </div>
          <p className="rd-energy muted" style={{ marginTop: 8 }}>Première moitié {signed(d.class_pace.first_half_gap_s)} par tour, seconde moitié {signed(d.class_pace.second_half_gap_s)}.</p>
        </div>
      )}

      {d.stints.length > 1 && (
        <p className="rd-energy">
          Rythme par relais :{" "}
          {d.stints.map((s, i) => `${i ? " · " : ""}tours ${s.from_lap}–${s.to_lap} ${lapTime(s.median_s)} (${lapTime(s.first_half_s)} puis ${lapTime(s.second_half_s)})`).join("")}
        </p>
      )}
      {d.stints.filter((s) => s.drift?.summary).map((s) => (
        <p key={s.from_lap} className="rd-energy muted">
          Relais {s.from_lap}–{s.to_lap}, première moitié contre seconde ({s.drift!.laps_first} et {s.drift!.laps_second} tours) : {s.drift!.summary}
        </p>
      ))}

      {(d.wear_events ?? []).length > 0 && (
        <div className="rd-class">
          <h3 className="rd-title">Usure anormale</h3>
          {d.wear_events.map((w) => (
            <p key={`${w.lap}-${w.wheel}`} className="rd-energy">{w.detail}</p>
          ))}
        </div>
      )}

      {d.missing.map((m) => (
        <p key={m} className="muted rd-missing">Non mesuré. {m}</p>
      ))}
    </div>
  );
}
