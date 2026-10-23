import { useEffect, useState } from "react";
import { api, onSessionsChanged } from "../lib/api";
import { dateTime } from "../lib/format";
import type { PlanView } from "../lib/types";

/** Programme d'entraînement de la prochaine séance (circuit + voiture), puis son bilan. */
export function PlanCard({ sessionId }: { sessionId: number }) {
  const [plan, setPlan] = useState<PlanView | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const load = () => api.trainingPlan(sessionId).then(setPlan, (e: unknown) => setErr(e instanceof Error ? e.message : String(e)));
  useEffect(() => {
    void load();
    const un = onSessionsChanged(() => void load());
    return () => void un.then((f) => f());
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sessionId]);
  const renew = () => api.newTrainingPlan(sessionId).then(setPlan, (e: unknown) => setErr(e instanceof Error ? e.message : String(e)));

  if (err) return <div className="error">{err}</div>;
  if (!plan) return null;
  const evaluated = plan.evaluated_session_id != null;
  const done = plan.objectives.filter((o) => o.result?.achieved).length;
  return (
    <div className="card plan" style={{ marginBottom: 14 }}>
      <div className="row" style={{ justifyContent: "space-between", alignItems: "baseline" }}>
        <h2 style={{ margin: 0 }}>{evaluated ? "Bilan du programme" : "Programme de ta prochaine séance"}</h2>
        <span className="muted" style={{ fontSize: 12 }}>{plan.track} · {plan.car} · créé le {dateTime(plan.created_at_unix)}</span>
      </div>
      {evaluated && <div className="risk-headline">{done} objectif{done > 1 ? "s" : ""} atteint{done > 1 ? "s" : ""} sur {plan.objectives.length}.</div>}
      <ol className="plan-list">
        {plan.objectives.map((o) => (
          <li key={o.title}>
            <div className="row" style={{ justifyContent: "space-between", gap: 8 }}>
              <b>{o.title}</b>
              {o.result ? <span className={`chip ${o.result.achieved ? "good" : "warn"}`}>{o.result.achieved ? "Atteint" : "Pas encore"}</span> : <span className="chip">À faire</span>}
            </div>
            <div className="muted" style={{ fontSize: 13 }}>{o.result ? o.result.summary : o.why}</div>
          </li>
        ))}
      </ol>
      <div className="row" style={{ justifyContent: "space-between", marginTop: 8 }}>
        <span className="muted" style={{ fontSize: 12 }}>
          {evaluated ? "Prochaine étape : un nouveau programme à partir de cette séance." : "Le coach vocal privilégie ces virages et te lit le plan au départ (« plan » au volant)."}
        </span>
        <button className={`btn ${evaluated ? "primary" : ""}`} onClick={() => void renew()}>{evaluated ? "Nouveau programme" : "Recalculer"}</button>
      </div>
    </div>
  );
}
