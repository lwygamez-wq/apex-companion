import { useState } from "react";
import { api } from "../lib/api";
import { CONFIDENCE_FR, PRIORITY_FR } from "../lib/format";
import type { Insight } from "../lib/types";

export function InsightCard({ insight, active, onFocus, sessionId }: { insight: Insight; active?: boolean; onFocus?: () => void; sessionId?: number }) {
  const g = insight.est_gain_s;
  const [goalMsg, setGoalMsg] = useState<string | null>(null);
  return (
    <div className="insight" style={active ? { borderColor: "var(--accent)" } : undefined} onClick={onFocus}>
      <div className="head">
        <span className={`chip ${insight.priority}`}>{PRIORITY_FR[insight.priority]}</span>
        {g && <span className="mono muted">gain estimé {g[0].toFixed(2)}–{g[1].toFixed(2)} s</span>}
      </div>
      <h3>{insight.title}</h3>
      <div>{insight.explanation}</div>
      <div className="rec">{insight.recommendation}</div>
      {insight.goal && sessionId != null && (
        <div className="row" style={{ marginTop: 8 }}>
          <button
            className="btn"
            onClick={(e) => {
              e.stopPropagation();
              api.createGoal(sessionId, insight.goal!).then(
                () => setGoalMsg("Objectif enregistré : il sera vérifié sur ta prochaine session ici."),
                (err: unknown) => setGoalMsg(err instanceof Error ? err.message : String(err)),
              );
            }}
          >
            Fixer comme objectif
          </button>
          <span className="muted" style={{ fontSize: 12 }}>{goalMsg ?? insight.goal.title}</span>
        </div>
      )}
      <details>
        <summary>Données et confiance ({CONFIDENCE_FR[insight.confidence]})</summary>
        <table>
          <thead>
            <tr><th>Mesure</th><th className="num">Réf.</th><th className="num">Tour</th><th className="num">Écart</th></tr>
          </thead>
          <tbody>
            {insight.evidence.map((e) => (
              <tr key={e.label}><td>{e.label}</td><td className="num">{e.reference}</td><td className="num">{e.compared}</td><td className="num">{e.delta}</td></tr>
            ))}
          </tbody>
        </table>
        {insight.confidence_notes.length > 0 && (
          <ul>{insight.confidence_notes.map((n) => <li key={n}>{n}</li>)}</ul>
        )}
      </details>
    </div>
  );
}
