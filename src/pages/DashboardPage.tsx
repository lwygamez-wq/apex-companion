import { useEffect, useState } from "react";
import type { Route } from "../App";
import { ImportActions } from "../components/ImportActions";
import { api, onSessionsChanged } from "../lib/api";
import { dateTime, delta, lapTime, SESSION_KIND_FR } from "../lib/format";
import { useAsync } from "../lib/hooks";

function Sparkline({ points }: { points: [number, number][] }) {
  if (points.length < 2) return <div className="muted">Au moins 2 sessions sur ce combo sont nécessaires.</div>;
  const ys = points.map((p) => p[1]);
  const min = Math.min(...ys), max = Math.max(...ys), span = max - min || 1;
  const d = points.map((p, i) => `${i === 0 ? "M" : "L"}${(i / (points.length - 1)) * 300},${60 - ((p[1] - min) / span) * 50 - 5}`).join(" ");
  return (
    <svg viewBox="0 0 300 60" style={{ width: "100%", height: 60 }}>
      <path d={d} stroke="#c9c9d4" strokeWidth={2} fill="none" />
    </svg>
  );
}

export function DashboardPage({ navigate }: { navigate: (r: Route) => void }) {
  const dash = useAsync(() => api.dashboard(), []);
  const autolog = useAsync(() => api.iracingAutolog().catch(() => null), []);
  const [msg, setMsg] = useState<string | null>(null);
  const run = (p: Promise<unknown>, ok: string) =>
    p.then(
      () => (setMsg(ok), dash.reload(), autolog.reload()),
      (e: unknown) => setMsg(e instanceof Error ? e.message : String(e)),
    );
  useEffect(() => {
    const un = onSessionsChanged(dash.reload);
    return () => void un.then((f) => f());
  }, [dash.reload]);

  const d = dash.data;
  const s = d?.last_session;
  return (
    <>
      <div className="page-title">
        <h1>Tableau de bord</h1>
        <button className="btn" onClick={() => navigate({ page: "live" })}>Ouvrir le Live Coach</button>
      </div>
      {dash.error && <div className="error">{dash.error}</div>}
      {msg && <div className="warning">{msg}</div>}
      {autolog.data?.enabled === false && (
        <div className="card" style={{ marginBottom: 14, borderColor: "#5c4a14" }}>
          <div className="row" style={{ justifyContent: "space-between" }}>
            <div>
              <b>iRacing n'enregistre tes sessions que si tu appuies sur Alt+L.</b>
              <div className="muted" style={{ fontSize: 12 }}>Active l'enregistrement automatique pour ne jamais perdre une session (iRacing doit être fermé). Environ 200 Mo par heure de roulage.</div>
            </div>
            <button className="btn primary" onClick={() => run(api.setIracingAutolog(true), "Enregistrement automatique activé : toutes tes sessions iRacing seront enregistrées.")}>Activer</button>
          </div>
        </div>
      )}
      {d && !s && (
        <div className="card empty">
          <h2>Aucune session enregistrée</h2>
          <p>Importe tes fichiers de télémétrie iRacing, ou charge une session de démonstration pour découvrir l'analyse.</p>
          <div className="row" style={{ justifyContent: "center" }}><ImportActions onDone={dash.reload} /></div>
        </div>
      )}
      {s && d && (
        <div className="grid" style={{ gap: 14 }}>
          <div className="grid cols-4">
            <div className="card">
              <h2>Dernière session</h2>
              <div style={{ fontWeight: 600 }}>{s.track}{s.track_config ? `, ${s.track_config}` : ""}</div>
              <div className="kpi-sub">{s.car}, {s.sim_name}</div>
              <div className="kpi-sub">{SESSION_KIND_FR[s.kind]}, {dateTime(s.started_at_unix)}</div>
            </div>
            <div className="card">
              <h2>Meilleur tour propre</h2>
              <div className="kpi best">{lapTime(s.best_lap_s)}</div>
              <div className="kpi-sub">{s.clean_lap_count} tour(s) propre(s) sur {s.lap_count}</div>
            </div>
            <div className="card">
              <h2>Temps théorique</h2>
              <div className="kpi best">{lapTime(d.theoretical_best_s)}</div>
              <div className="kpi-sub">
                {d.theoretical_best_s != null && s.best_lap_s != null ? `${delta(d.theoretical_best_s - s.best_lap_s)} s vs meilleur tour` : "Au moins 2 tours propres requis"}
              </div>
            </div>
            <div className="card">
              <h2>Régularité</h2>
              <div className="kpi">{s.stddev_clean_s != null ? `σ ${s.stddev_clean_s.toFixed(3)} s` : "—"}</div>
              <div className="kpi-sub">{s.mean_clean_s != null ? `moyenne ${lapTime(s.mean_clean_s)}` : "tours propres insuffisants"}</div>
            </div>
          </div>

          {d.active_goal && (
            <div className="card" style={{ borderColor: d.active_goal.result?.achieved ? "#1c5234" : "#24476b" }}>
              <div className="row" style={{ justifyContent: "space-between" }}>
                <h2 style={{ margin: 0 }}>Objectif en cours</h2>
                <button className="btn" onClick={() => run(api.archiveGoal(d.active_goal!.id), "Objectif clôturé.")}>Clôturer</button>
              </div>
              <div style={{ fontWeight: 600, marginTop: 8 }}>{d.active_goal.title}</div>
              {d.active_goal.result ? (
                <div style={{ marginTop: 6 }}>
                  <span className={`chip ${d.active_goal.result.achieved ? "good" : "warn"}`}>{d.active_goal.result.achieved ? "Atteint" : "Pas encore"}</span>{" "}
                  <span className="muted">{d.active_goal.result.summary}</span>
                </div>
              ) : (
                <div className="muted" style={{ marginTop: 6 }}>Sera vérifié sur ta prochaine session sur ce circuit.</div>
              )}
            </div>
          )}

          {d.next_goal && (
            <div className="card" style={{ borderColor: "#5c2a1e" }}>
              <h2>Prochain objectif</h2>
              <div>{d.next_goal}</div>
              {d.next_goal_suggestion && !d.active_goal && (
                <button className="btn" style={{ marginTop: 10 }} onClick={() => run(api.createGoal(s.id, d.next_goal_suggestion!), "Objectif enregistré.")}>
                  Fixer comme objectif : {d.next_goal_suggestion.title}
                </button>
              )}
            </div>
          )}

          <div className="grid cols-2">
            <div className="card">
              <h2>Principales zones de progrès</h2>
              {d.progress_zones.length === 0 && <div className="muted">Aucune perte récurrente détectée sur la dernière session.</div>}
              <table>
                <thead><tr><th>Virage</th><th>Problème type</th><th className="num">Récurrence</th><th className="num">Perte moy.</th></tr></thead>
                <tbody>
                  {d.progress_zones.slice(0, 6).map((z) => (
                    <tr key={z.segment} className="clickable" onClick={() => navigate({ page: "analysis", sessionId: s.id })}>
                      <td className="mono">{z.segment}</td>
                      <td>{z.typical_issue.split(" : ")[1] ?? z.typical_issue}</td>
                      <td className="num">{z.occurrences}/{z.laps_compared}</td>
                      <td className="num">{z.avg_loss_s.toFixed(2)} s</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            <div className="card">
              <h2>Évolution de la régularité (σ, même combo)</h2>
              <Sparkline points={d.consistency_trend} />
              <div className="kpi-sub">{d.total_sessions} session(s) au total</div>
              <div style={{ marginTop: 14 }}><ImportActions onDone={dash.reload} /></div>
            </div>
          </div>
        </div>
      )}
    </>
  );
}
