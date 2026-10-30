import { delta, lapTime } from "../lib/format";
import type { LapRow, RaceReport } from "../lib/types";

const KIND_CLS: Record<RaceReport["insights"][number]["kind"], string> = { danger: "bad", warning: "warn", good: "good", info: "" };

/** Positions tour par tour (P1 en haut), incidents marqués sur le tour où ils ont eu lieu. */
function PositionChart({ r }: { r: RaceReport }) {
  if (r.positions.length < 2) return null;
  const W = 900, H = 200, P = { l: 40, r: 14, t: 12, b: 24 };
  const maxLap = Math.max(...r.positions.map((p) => p[0]), ...r.incidents.map((i) => i.lap));
  const worst = Math.max(r.field_size, ...r.positions.map((p) => p[1]));
  const x = (lap: number) => P.l + (lap / Math.max(1, maxLap)) * (W - P.l - P.r);
  const y = (pos: number) => P.t + ((pos - 1) / Math.max(1, worst - 1)) * (H - P.t - P.b);
  const path = r.positions.map(([l, p], i) => `${i ? "L" : "M"}${x(l).toFixed(1)},${y(p).toFixed(1)}`).join(" ");
  const ticks = Array.from(new Set([1, Math.ceil(worst / 4), Math.ceil(worst / 2), Math.ceil((3 * worst) / 4), worst]));
  // Un marqueur par tour avec incident (points cumulés), placé sur la position de ce tour.
  const byLap = new Map<number, number>();
  r.incidents.forEach((i) => byLap.set(i.lap, (byLap.get(i.lap) ?? 0) + i.points));
  const posAt = (lap: number) => (r.positions.find((p) => p[0] >= lap) ?? r.positions[r.positions.length - 1]!)[1];
  return (
    <svg viewBox={`0 0 ${W} ${H}`} className="race-chart" role="img" aria-label="Position tour par tour">
      {ticks.map((p) => (
        <g key={p}>
          <line x1={P.l} x2={W - P.r} y1={y(p)} y2={y(p)} stroke="#25252f" />
          <text x={P.l - 8} y={y(p) + 4} textAnchor="end" fontSize={11} fill="#8c8c9c">P{p}</text>
        </g>
      ))}
      <path d={path} stroke="#ffffff" strokeWidth={2.2} fill="none" />
      {r.positions.map(([l, p]) => <circle key={l} cx={x(l)} cy={y(p)} r={2.6} fill="#ffffff" />)}
      {[...byLap.entries()].map(([lap, pts]) => (
        <g key={lap}>
          <circle cx={x(lap)} cy={y(posAt(lap))} r={pts >= 4 ? 8 : pts >= 2 ? 6.5 : 5} fill="none" stroke={pts >= 4 ? "#ff5a4e" : "#ff8a00"} strokeWidth={2} />
          <text x={x(lap)} y={y(posAt(lap)) - 10} textAnchor="middle" fontSize={11} fill={pts >= 4 ? "#ff5a4e" : "#ff8a00"}>{pts}x</text>
        </g>
      ))}
      <text x={P.l} y={H - 6} fontSize={11} fill="#8c8c9c">grille</text>
      <text x={W - P.r} y={H - 6} textAnchor="end" fontSize={11} fill="#8c8c9c">tour {maxLap}</text>
    </svg>
  );
}

/** Coach de course : bilan, positions, incidents et points à travailler. */
export function RaceCard({ r, laps, onLap }: { r: RaceReport; laps: LapRow[]; onLap: (lapId: number) => void }) {
  const lapId = (n: number) => laps.find((l) => l.number === n && l.lap_time_s != null)?.id;
  const gained = r.positions_gained;
  return (
    <div className="card race" style={{ marginBottom: 14 }}>
      <div className="row" style={{ justifyContent: "space-between", alignItems: "baseline" }}>
        <h2>Coach de course</h2>
        <span className="muted" style={{ fontSize: 12 }}>{r.field_size} pilotes{r.field_irating ? ` · iRating moyen ${r.field_irating}` : ""}</span>
      </div>
      <div className="risk-headline">{r.headline}</div>
      <div className="grid cols-4" style={{ marginBottom: 10 }}>
        <div className="race-kpi">
          <span className="muted">Grille → arrivée</span>
          <b>{r.start_position ? `P${r.start_position}` : "—"} → {r.finish_position ? `P${r.finish_position}` : "—"}</b>
          {gained != null && <span className={gained > 0 ? "good-txt" : gained < 0 ? "pos" : "muted"}>{gained > 0 ? `+${gained} places` : gained < 0 ? `${gained} places` : "position gardée"}</span>}
        </div>
        <div className="race-kpi">
          <span className="muted">Incidents</span>
          <b>{r.incident_total}x</b>
          <span className="muted">{r.incidents.filter((i) => i.points > 0).length} comptés</span>
        </div>
        <div className="race-kpi">
          <span className="muted">Qualification</span>
          <b>{r.my_qualify_s != null && r.pole_s != null ? `${delta(r.my_qualify_s - r.pole_s)} s` : "—"}</b>
          <span className="muted">{r.race_lap_as_qualify ? `ton rythme course = P${r.race_lap_as_qualify}` : "de la pole"}</span>
        </div>
        <div className="race-kpi">
          <span className="muted">Rythme course</span>
          <b>{r.pace ? lapTime(r.pace.median_s) : "—"}</b>
          <span className="muted">{r.pace ? `médian · meilleur ${lapTime(r.pace.best_s)} · ±${r.pace.spread_s.toFixed(2)} s` : "pas assez de tours propres"}</span>
        </div>
      </div>
      <PositionChart r={r} />
      {r.insights.length > 0 && (
        <div className="race-insights">
          {r.insights.map((i) => (
            <div key={i.title} className={`race-insight ${i.kind}`}>
              <div className="risk-main"><b>{i.title}</b><span className={`chip ${KIND_CLS[i.kind]}`}>{i.kind === "danger" ? "priorité" : i.kind === "warning" ? "à travailler" : i.kind === "good" ? "acquis" : "info"}</span></div>
              <div className="risk-msg">{i.detail}</div>
            </div>
          ))}
        </div>
      )}
      {r.incidents.length > 0 && (
        <table style={{ marginTop: 10 }}>
          <thead><tr><th className="num">Tour</th><th>Où</th><th>Quoi</th><th className="num">Temps perdu sur le tour</th><th /></tr></thead>
          <tbody>
            {r.incidents.map((i, k) => {
              const id = lapId(i.lap);
              return (
                <tr key={k}>
                  <td className="num">{i.lap}</td>
                  <td>{i.corner ?? `${Math.round(i.dist_m)} m`}</td>
                  <td>{i.label}</td>
                  <td className="num">{i.lap_loss_s != null ? `+${i.lap_loss_s.toFixed(2)} s` : "—"}</td>
                  <td>{id != null && <button className="btn" onClick={() => onLap(id)} title="Comparer ce tour à ta référence">Voir le tour</button>}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      )}
      <div className="muted" style={{ fontSize: 12, marginTop: 8 }}>
        L'enregistrement ne contient que ta voiture : positions relevées au passage de la ligne, sans écarts ni lieu des dépassements. Temps perdu = écart à ton tour médian propre.
      </div>
    </div>
  );
}
