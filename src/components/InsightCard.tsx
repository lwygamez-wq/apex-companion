import { useState } from "react";
import { api } from "../lib/api";
import { CONFIDENCE_FR, PRIORITY_FR } from "../lib/format";
import type { Insight } from "../lib/types";

/**
 * Un conseil, lisible en 3 secondes : le virage, le problème, ce qu'il faut faire.
 * L'explication et les mesures restent à un clic (« Pourquoi »).
 */
export function InsightCard({ insight, active, onFocus, sessionId, rank }: { insight: Insight; active?: boolean; onFocus?: () => void; sessionId?: number; rank?: number }) {
  const g = insight.est_gain_s;
  const [goalMsg, setGoalMsg] = useState<string | null>(null);
  const [where, what] = insight.title.includes(" : ") ? insight.title.split(" : ", 2) : [insight.segment ?? "", insight.title];
  return (
    <div className={`insight ${insight.priority}`} style={active ? { borderColor: "var(--accent)" } : undefined} onClick={onFocus}>
      <div className="head">
        {rank != null && <span className="insight-rank">{rank}</span>}
        <div className="insight-title">
          <b>{where}</b>
          <span>{what}</span>
        </div>
        {g && <span className="insight-gain mono" title={`Priorité : ${PRIORITY_FR[insight.priority]}`}>≈ {g[0].toFixed(1)}–{g[1].toFixed(1)} s</span>}
      </div>
      <div className="rec">{insight.recommendation}</div>
      <div className="insight-foot">
        <details onClick={(e) => e.stopPropagation()}>
          <summary>Pourquoi</summary>
          <p>{insight.explanation}</p>
          <table>
            <thead>
              <tr><th>Mesure</th><th className="num">Réf.</th><th className="num">Toi</th><th className="num">Écart</th></tr>
            </thead>
            <tbody>
              {insight.evidence.map((e) => (
                <tr key={e.label}><td>{e.label}</td><td className="num">{e.reference}</td><td className="num">{e.compared}</td><td className="num">{e.delta}</td></tr>
              ))}
            </tbody>
          </table>
          <div className="muted" style={{ fontSize: 12 }}>Confiance : {CONFIDENCE_FR[insight.confidence]}{insight.confidence_notes.length > 0 ? ` · ${insight.confidence_notes.join(" · ")}` : ""}</div>
        </details>
        {insight.goal && sessionId != null && (
          <button
            className="btn ghost small"
            title={insight.goal.title}
            disabled={goalMsg != null}
            onClick={(e) => {
              e.stopPropagation();
              api.addPlanObjective(sessionId, insight.goal!).then(
                () => setGoalMsg("ajouté"),
                (err: unknown) => setGoalMsg(err instanceof Error ? err.message : String(err)),
              );
            }}
          >
            {goalMsg === "ajouté" ? "✓ Dans le programme" : goalMsg ?? "+ Programme"}
          </button>
        )}
      </div>
    </div>
  );
}
