import { useEffect, useMemo, useState } from "react";
import type { Route } from "../App";
import { api } from "../lib/api";
import { dateTime, delta, lapTime, SESSION_KIND_FR } from "../lib/format";
import { useAsync } from "../lib/hooks";
import type { CircuitOverview, HistoryLap } from "../lib/types";
import { SetupChip } from "../components/SetupChip";

const NAMES_FR: Record<CircuitOverview["corner_names"], string> = {
  exact: "virages nommés (relevé)",
  aligned: "virages nommés",
  none: "virages numérotés",
};

const key = (c: CircuitOverview) => `${c.sim}|${c.track}|${c.track_config ?? ""}|${c.car}|${c.setup ?? ""}`;

/** Nuage des temps au tour dans l'ordre chronologique, record propre en violet. */
function LapChart({ laps, best, external }: { laps: HistoryLap[]; best: number | null; external: [string, number] | null }) {
  const timed = laps.filter((l) => l.kind === "flying" && l.lap_time_s != null);
  if (timed.length < 2 || best == null) return null;
  const W = 900, H = 220, P = { l: 56, r: 16, t: 14, b: 26 };
  // Échelle bornée : les tours ratés (> 107 % du record) sont écrêtés en haut pour garder la lisibilité.
  const lo = Math.min(best, external?.[1] ?? best) - 0.4;
  const hi = Math.max(best * 1.07, lo + 2);
  const x = (i: number) => P.l + (i / Math.max(1, timed.length - 1)) * (W - P.l - P.r);
  const y = (t: number) => P.t + ((Math.min(t, hi) - lo) / (hi - lo)) * (H - P.t - P.b);
  const ticks = Array.from({ length: 5 }, (_, k) => lo + ((hi - lo) * k) / 4);
  // Séparateurs de session.
  const seps: number[] = [];
  timed.forEach((l, i) => {
    if (i > 0 && l.session_id !== timed[i - 1]!.session_id) seps.push(i);
  });
  // Évolution du record propre (escalier).
  let run = Infinity;
  const pts: string[] = [];
  timed.forEach((l, i) => {
    if (l.clean && l.lap_time_s! < run) run = l.lap_time_s!;
    if (Number.isFinite(run)) pts.push(`${x(i).toFixed(1)},${y(run).toFixed(1)}`);
  });
  const pbPath = pts.map((p, i) => `${i ? "L" : "M"}${p}`).join(" ");
  return (
    <svg viewBox={`0 0 ${W} ${H}`} className="history-chart" role="img" aria-label="Temps au tour dans l'ordre chronologique">
      {ticks.map((t) => (
        <g key={t}>
          <line x1={P.l} x2={W - P.r} y1={y(t)} y2={y(t)} stroke="#25252f" />
          <text x={P.l - 8} y={y(t) + 4} textAnchor="end" fontSize={11} fill="#8c8c9c">{lapTime(t)}</text>
        </g>
      ))}
      {seps.map((i) => (
        <line key={i} x1={(x(i) + x(i - 1)) / 2} x2={(x(i) + x(i - 1)) / 2} y1={P.t} y2={H - P.b} stroke="#383848" strokeDasharray="3 4" />
      ))}
      {external && (
        <g>
          <line x1={P.l} x2={W - P.r} y1={y(external[1])} y2={y(external[1])} stroke="#a259ff" strokeDasharray="6 5" opacity={0.7} />
          <text x={W - P.r} y={y(external[1]) - 5} textAnchor="end" fontSize={11} fill="#c49bff">{external[0]} {lapTime(external[1])}</text>
        </g>
      )}
      <path d={pbPath} stroke="#a259ff" strokeWidth={2} fill="none" />
      {timed.map((l, i) => (
        <circle
          key={l.lap_id}
          cx={x(i)}
          cy={y(l.lap_time_s!)}
          r={l.personal_best_at_time ? 4.5 : 3.2}
          fill={l.personal_best_at_time ? "#a259ff" : l.clean ? "#ffffff" : "none"}
          stroke={l.clean ? "none" : l.off_track ? "#ff8a00" : "#8c8c9c"}
          strokeWidth={1.5}
        >
          <title>{`${dateTime(l.started_at_unix)} · tour ${l.number} · ${lapTime(l.lap_time_s)}${l.clean ? "" : l.off_track ? " · hors piste" : " · perturbé"}`}</title>
        </circle>
      ))}
      <text x={P.l} y={H - 6} fontSize={11} fill="#8c8c9c">plus ancien</text>
      <text x={W - P.r} y={H - 6} textAnchor="end" fontSize={11} fill="#8c8c9c">plus récent</text>
    </svg>
  );
}

/** Historique de tous les tours, circuit par circuit (circuit + configuration + voiture). */
export function HistoryPage({ navigate }: { navigate: (r: Route) => void }) {
  const circuits = useAsync(() => api.circuits(), []);
  const [sel, setSel] = useState<string | null>(null);
  const [onlyClean, setOnlyClean] = useState(false);
  const [sort, setSort] = useState<"date" | "time">("date");
  const current = useMemo(() => circuits.data?.find((c) => key(c) === sel) ?? circuits.data?.[0] ?? null, [circuits.data, sel]);
  const history = useAsync(
    () => (current ? api.lapHistory(current.sim, current.track, current.track_config, current.car, current.setup) : Promise.resolve([] as HistoryLap[])),
    [current ? key(current) : ""],
  );
  useEffect(() => setOnlyClean(false), [sel]);

  const rows = useMemo(() => {
    let r = (history.data ?? []).filter((l) => l.lap_time_s != null);
    if (onlyClean) r = r.filter((l) => l.clean);
    r = [...r];
    if (sort === "time") r.sort((a, b) => a.lap_time_s! - b.lap_time_s!);
    else r.reverse();
    return r;
  }, [history.data, onlyClean, sort]);

  const firstClean = (history.data ?? []).find((l) => l.clean && l.kind === "flying" && l.lap_time_s != null)?.lap_time_s ?? null;

  return (
    <>
      <div className="page-title"><h1>Historique</h1></div>
      {circuits.error && <div className="error">{circuits.error}</div>}
      {circuits.data?.length === 0 && <div className="card empty"><h2>Aucun circuit</h2><p>Roule une session iRacing : elle apparaîtra ici automatiquement.</p></div>}
      {current && (
        <div className="history">
          <div className="history-list">
            {circuits.data!.map((c) => (
              <button key={key(c)} className={`history-item ${key(c) === key(current) ? "on" : ""}`} onClick={() => setSel(key(c))}>
                <b>{c.track} <SetupChip setup={c.setup} /></b>
                <span className="muted">{c.track_config ?? ""}{c.track_config ? " · " : ""}{c.car}</span>
                <span className="history-item-row">
                  <span className={c.best_clean_s != null ? "kpi best history-kpi" : "kpi history-kpi"}>{lapTime(c.best_clean_s ?? c.best_any_s)}</span>
                  <span className="muted">{c.sessions} session{c.sessions > 1 ? "s" : ""} · {c.timed_laps} tours</span>
                </span>
              </button>
            ))}
          </div>
          <div className="history-main">
            <div className="grid cols-4" style={{ marginBottom: 12 }}>
              <div className="card"><h2>Record propre</h2><div className="kpi best">{lapTime(current.best_clean_s)}</div>{current.best_clean_s == null && <div className="kpi-sub">aucun tour propre</div>}</div>
              <div className="card"><h2>Progression</h2><div className="kpi">{firstClean != null && current.best_clean_s != null ? `${delta(current.best_clean_s - firstClean)} s` : "—"}</div><div className="kpi-sub">depuis ton 1er tour propre ici</div></div>
              <div className="card"><h2>Tours propres</h2><div className="kpi">{current.timed_laps ? `${Math.round((current.clean_laps / current.timed_laps) * 100)} %` : "—"}</div><div className="kpi-sub">{current.clean_laps} sur {current.timed_laps} tours lancés</div></div>
              <div className="card">
                <h2>Référence externe</h2>
                <div className="kpi" style={{ fontSize: 26 }}>{current.external_reference ? lapTime(current.external_reference[1]) : "—"}</div>
                <div className="kpi-sub">{current.external_reference ? current.external_reference[0] : "aucune (export Garage 61)"}</div>
              </div>
            </div>
            <div className="card" style={{ marginBottom: 12 }}>
              <div className="row" style={{ justifyContent: "space-between" }}>
                <h2>Tous tes tours, du plus ancien au plus récent</h2>
                <span className="legend">
                  <span><i style={{ background: "#a259ff" }} />record (évolution)</span>
                  <span><i style={{ background: "#ffffff" }} />propre</span>
                  <span><i style={{ background: "#ff8a00" }} />hors piste</span>
                </span>
              </div>
              <LapChart laps={history.data ?? []} best={current.best_clean_s ?? current.best_any_s} external={current.external_reference} />
              <div className="muted" style={{ fontSize: 12 }}>
                {current.sessions} session{current.sessions > 1 ? "s" : ""} du {dateTime(current.first_unix)} au {dateTime(current.last_unix)} · {NAMES_FR[current.corner_names]}
              </div>
            </div>
            <div className="card" style={{ padding: 0 }}>
              <div className="row" style={{ justifyContent: "space-between", padding: "10px 12px" }}>
                <div className="seg">
                  <button className={sort === "date" ? "on" : ""} onClick={() => setSort("date")}>Par date</button>
                  <button className={sort === "time" ? "on" : ""} onClick={() => setSort("time")}>Par temps</button>
                </div>
                <label className="muted" style={{ fontSize: 13 }}>
                  <input type="checkbox" checked={onlyClean} onChange={(e) => setOnlyClean(e.target.checked)} /> tours propres seulement
                </label>
              </div>
              <table>
                <thead>
                  <tr><th>Date</th><th>Session</th><th className="num">Tour</th><th className="num">Temps</th><th className="num">Écart au record</th><th>État</th><th /></tr>
                </thead>
                <tbody>
                  {rows.map((l) => (
                    <tr key={l.lap_id}>
                      <td>{dateTime(l.started_at_unix)}</td>
                      <td className="muted">{SESSION_KIND_FR[l.session_kind]}</td>
                      <td className="num">{l.number}</td>
                      <td className="num">
                        <span className={current.best_clean_s != null && l.clean && Math.abs(l.lap_time_s! - current.best_clean_s) < 1e-6 ? "kpi best history-time" : "history-time"}>{lapTime(l.lap_time_s)}</span>
                      </td>
                      <td className={`num ${l.delta_to_best_s == null ? "" : l.delta_to_best_s > 0.0005 ? "pos" : ""}`}>{l.delta_to_best_s == null ? "—" : Math.abs(l.delta_to_best_s) < 0.0005 ? "record" : `${delta(l.delta_to_best_s)} s`}</td>
                      <td>
                        {l.personal_best_at_time && <span className="chip best">nouveau record</span>}{" "}
                        {l.kind !== "flying" ? <span className="chip">{l.kind === "out_lap" ? "sortie des stands" : l.kind === "in_lap" ? "entrée aux stands" : "interrompu"}</span>
                          : l.clean ? <span className="chip good">propre</span>
                          : l.off_track ? <span className="chip warn">hors piste</span>
                          : <span className="chip">perturbé</span>}
                        {l.is_reference && <span className="chip warn" style={{ marginLeft: 4 }}>référence</span>}
                      </td>
                      <td><button className="btn" onClick={() => navigate({ page: "analysis", sessionId: l.session_id, cmpLapId: l.lap_id })}>Analyser</button></td>
                    </tr>
                  ))}
                </tbody>
              </table>
              {rows.length === 0 && <div className="empty">Aucun tour{onlyClean ? " propre" : ""}.</div>}
            </div>
          </div>
        </div>
      )}
    </>
  );
}
