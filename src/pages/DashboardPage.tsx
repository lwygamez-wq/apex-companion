import { useEffect, useState } from "react";
import type { Route } from "../App";
import { ImportActions } from "../components/ImportActions";
import { PlanCard } from "../components/PlanCard";
import { api, onSessionsChanged } from "../lib/api";
import { dateTime, lapTime, SESSION_KIND_FR } from "../lib/format";
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
        <h1>Accueil</h1>
        {s && <ImportActions compact onDone={dash.reload} />}
      </div>
      {dash.error && <div className="error">{dash.error}</div>}
      {msg && <div className="warning" onClick={() => setMsg(null)}>{msg}</div>}
      {autolog.data?.enabled === false && (
        <div className="card notice">
          <div>
            <b>iRacing n'enregistre tes sessions que si tu appuies sur Alt+L.</b>
            <div className="muted" style={{ fontSize: 12 }}>Active l'enregistrement automatique (iRacing fermé). Environ 200 Mo par heure de roulage.</div>
          </div>
          <button className="btn primary" onClick={() => run(api.setIracingAutolog(true), "Enregistrement automatique activé.")}>Activer</button>
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
        <>
          <h3 className="section-title">Dernière session</h3>
          <div className="session-strip clickable" role="button" tabIndex={0} onClick={() => navigate({ page: "analysis", sessionId: s.id })} onKeyDown={(e) => e.key === "Enter" && navigate({ page: "analysis", sessionId: s.id })}>
            <div className="ss-where">
              <b>{s.track}</b>
              <span className="muted">{s.car} · {SESSION_KIND_FR[s.kind]} · {dateTime(s.started_at_unix)}</span>
              <span className="link-like">Analyser →</span>
            </div>
            <div className="ss-kpi"><span>Meilleur tour propre</span><b className="mono best">{lapTime(s.best_lap_s)}</b></div>
            <div className="ss-kpi"><span>Théorique</span><b className="mono best">{lapTime(d.theoretical_best_s)}</b></div>
            <div className="ss-kpi"><span>Régularité</span><b className="mono">{s.stddev_clean_s != null ? `± ${s.stddev_clean_s.toFixed(2)} s` : "—"}</b></div>
            <div className="ss-kpi"><span>Tours propres</span><b className="mono">{s.clean_lap_count}/{s.lap_count}</b></div>
          </div>

          {d.active_goal && (
            <div className="card notice">
              <div>
                <span className="muted" style={{ fontSize: 12 }}>Objectif en cours</span>
                <div style={{ fontWeight: 650 }}>{d.active_goal.title}</div>
                <div className="muted" style={{ fontSize: 13 }}>
                  {d.active_goal.result ? <><span className={`chip ${d.active_goal.result.achieved ? "good" : "warn"}`}>{d.active_goal.result.achieved ? "Atteint" : "Pas encore"}</span> {d.active_goal.result.summary}</> : "Vérifié sur ta prochaine session sur ce circuit."}
                </div>
              </div>
              <button className="btn ghost small" onClick={() => run(api.archiveGoal(d.active_goal!.id), "Objectif clôturé.")}>Clôturer</button>
            </div>
          )}

          <PlanCard sessionId={s.id} />

          {(d.progress_zones.length > 0 || d.consistency_trend.length >= 2) && (
            <div className="grid cols-2" style={{ alignItems: "start" }}>
              {d.progress_zones.length > 0 && (
                <div className="card">
                  <h2>Où tu perds du temps à chaque tour</h2>
                  {d.progress_zones.slice(0, 4).map((z) => (
                    <div key={z.segment} className="zone-row clickable" onClick={() => navigate({ page: "analysis", sessionId: s.id })}>
                      <b>{z.segment}</b>
                      <span className="muted">{z.typical_issue.split(" : ")[1] ?? z.typical_issue}</span>
                      <span className="mono">{z.avg_loss_s.toFixed(2)} s</span>
                      <span className="muted" style={{ fontSize: 12 }}>{z.occurrences}/{z.laps_compared} tours</span>
                    </div>
                  ))}
                </div>
              )}
              {d.consistency_trend.length >= 2 && (
                <div className="card">
                  <h2>Régularité sur ce circuit (plus bas = mieux)</h2>
                  <Sparkline points={d.consistency_trend} />
                </div>
              )}
            </div>
          )}
        </>
      )}
    </>
  );
}
