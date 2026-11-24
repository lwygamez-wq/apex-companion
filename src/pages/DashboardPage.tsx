import { useEffect, useState } from "react";
import type { Route } from "../App";
import { ImportActions } from "../components/ImportActions";
import { NoCleanLaps } from "../components/NoCleanLaps";
import { PlanCard } from "../components/PlanCard";
import { api, onSessionsChanged } from "../lib/api";
import { dateTime, lapTime, SESSION_KIND_FR } from "../lib/format";
import { PHASE_SPOKEN } from "../lib/compareView";
import { useAsync } from "../lib/hooks";
import type { StatusPayload } from "../lib/types";
import { ReadyCard } from "../components/ReadyCard";
import { Alert, Button, EmptyState, LoadError, Skeleton, Stat } from "../components/ui";
import { AttackCard } from "../components/AttackCard";

function Sparkline({ points }: { points: [number, number][] }) {
  if (points.length < 2) return <div className="muted">Au moins 2 sessions sur ce combo sont nécessaires.</div>;
  const ys = points.map((p) => p[1]);
  const min = Math.min(...ys), max = Math.max(...ys), span = max - min || 1;
  const d = points.map((p, i) => `${i === 0 ? "M" : "L"}${(i / (points.length - 1)) * 300},${60 - ((p[1] - min) / span) * 50 - 5}`).join(" ");
  return (
    <svg viewBox="0 0 300 60" style={{ width: "100%", height: 60 }}>
      <path d={d} stroke="var(--ref)" strokeWidth={2} fill="none" />
    </svg>
  );
}

export function DashboardPage({ navigate, status }: { navigate: (r: Route) => void; status: StatusPayload | null }) {
  const dash = useAsync(() => api.dashboard(), []);
  const autolog = useAsync(() => api.iracingAutolog().catch(() => null), []);
  const plan = useAsync(() => (dash.data?.last_session ? api.trainingPlan(dash.data.last_session.id).catch(() => null) : Promise.resolve(null)), [dash.data?.last_session?.id]);
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
        <h1>Briefing</h1>
        {s && (
          <div className="row">
            <ImportActions compact onDone={dash.reload} />
            <Button variant="primary" onClick={() => navigate({ page: "analysis", sessionId: s.id })}>Analyser ma dernière séance</Button>
          </div>
        )}
      </div>
      {dash.error && <LoadError error={dash.error} onRetry={dash.reload} />}
      {dash.loading && !d && <Skeleton block lines={4} label="Chargement du briefing" />}
      {msg && <Alert action={<Button variant="quiet" onClick={() => setMsg(null)}>OK</Button>}>{msg}</Alert>}
      {d && !s && (
        <EmptyState title="Aucune séance enregistrée" action={<ImportActions onDone={dash.reload} />}>
          Lance Le Mans Ultimate avec Apex ouvert : chaque séance est enregistrée et analysée toute seule. Tu peux aussi importer des fichiers iRacing ou charger une séance de démonstration.
        </EmptyState>
      )}
      {s && d && (
        <>
          {plan.data?.objectives[0] && !plan.data.evaluated_session_id && (() => {
            const o = plan.data.objectives[0]!;
            const [where, what] = o.title.includes(" : ") ? o.title.split(" : ", 2) : ["", o.title];
            return (
              <section className="pitboard one-thing" aria-label="Consigne du prochain run">
                <div className="pitboard-head">
                  <span>Ton prochain run : une seule chose</span>
                </div>
                <h2 className="m-0">
                  {where && <span className="pitboard-slat">{where}</span>}
                  <span className="pitboard-slat call">{what}</span>
                </h2>
                <p className="pitboard-why">{o.why}</p>
              </section>
            );
          })()}
          <AttackCard sessionId={s.id} onMap={() => navigate({ page: "analysis", sessionId: s.id })} />
          <div className="briefing-top">
            <PlanCard sessionId={s.id} />
            <ReadyCard status={status} onSetup={() => navigate({ page: "live" })} />
          </div>

          <h3 className="section-title">Dernière séance</h3>
          <div className="session-strip clickable" role="button" tabIndex={0} onClick={() => navigate({ page: "analysis", sessionId: s.id })} onKeyDown={(e) => e.key === "Enter" && navigate({ page: "analysis", sessionId: s.id })}>
            <div className="ss-where">
              <b>{s.track}</b>
              <span className="muted">{s.car} · {SESSION_KIND_FR[s.kind]} · {dateTime(s.started_at_unix)}</span>
              <span className="link-like">Analyser →</span>
            </div>
            <Stat label="Meilleur tour propre" value={lapTime(s.best_lap_s)} tone="best" />
            <Stat label="Tour idéal" value={lapTime(d.theoretical_best_s)} tone="best" title="Tes meilleurs secteurs de la séance mis bout à bout" />
            <Stat label="Régularité" value={s.stddev_clean_s != null ? `± ${s.stddev_clean_s.toFixed(2)} s` : "—"} title="De combien tes tours propres varient d'un tour à l'autre (plus petit = plus régulier)" />
            <Stat label="Tours propres" value={`${s.clean_lap_count}/${s.lap_count}`} />
          </div>
          <NoCleanLaps clean={s.clean_lap_count} timed={s.lap_count} />

          {d.active_goal && (
            <div className="card notice">
              <div>
                <span className="muted t-sm">Objectif en cours</span>
                <div style={{ fontWeight: 650 }}>{d.active_goal.title}</div>
                <div className="muted t-sm">
                  {d.active_goal.result ? <><span className={`chip ${d.active_goal.result.achieved ? "good" : "warn"}`}>{d.active_goal.result.achieved ? "Atteint" : "Pas encore"}</span> {d.active_goal.result.summary}</> : "Vérifié sur ta prochaine session sur ce circuit."}
                </div>
              </div>
              <button className="btn ghost small" onClick={() => run(api.archiveGoal(d.active_goal!.id), "Objectif clôturé.")}>Clôturer</button>
            </div>
          )}

          {(d.progress_zones.length > 0 || d.consistency_trend.length >= 2) && (
            <div className="grid cols-2" style={{ alignItems: "start" }}>
              {d.progress_zones.length > 0 && (
                <div className="card">
                  <h2>Où tu perds du temps à chaque tour</h2>
                  {d.progress_zones.slice(0, 4).map((z) => (
                    <div key={z.segment} className="zone-row clickable" onClick={() => navigate({ page: "analysis", sessionId: s.id })}>
                      <b>{z.segment}</b>
                      <span className="muted">
                        {z.typical_issue.split(" : ")[1] ?? z.typical_issue}
                        {z.main_phase && <> · surtout {PHASE_SPOKEN[z.main_phase.phase]} ({z.main_phase.median_s.toFixed(2)} s, {z.main_phase.laps_losing}/{z.main_phase.laps} tours)</>}
                      </span>
                      <span className="mono">{z.avg_loss_s.toFixed(2)} s</span>
                      <span className="muted t-sm">{z.occurrences}/{z.laps_compared} tours</span>
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
      {autolog.data?.enabled === false && (
        <Alert title="iRacing : enregistrement automatique désactivé" action={<Button onClick={() => run(api.setIracingAutolog(true), "Enregistrement automatique activé.")}>Activer</Button>}>
          Sans lui, iRacing n'enregistre une séance que si tu appuies sur Alt+L. Environ 200 Mo par heure de roulage ; à activer iRacing fermé.
        </Alert>
      )}
    </>
  );
}
