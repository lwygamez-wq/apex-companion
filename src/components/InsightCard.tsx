import { CONFIDENCE_FR, PRIORITY_FR } from "../lib/format";
import type { Insight } from "../lib/types";

export function InsightCard({ insight, active, onFocus }: { insight: Insight; active?: boolean; onFocus?: () => void }) {
  const g = insight.est_gain_s;
  return (
    <div className="insight" style={active ? { borderColor: "var(--accent)" } : undefined} onClick={onFocus}>
      <div className="head">
        <span className={`chip ${insight.priority}`}>{PRIORITY_FR[insight.priority]}</span>
        {g && <span className="mono muted">gain estimé {g[0].toFixed(2)}–{g[1].toFixed(2)} s</span>}
      </div>
      <h3>{insight.title}</h3>
      <div>{insight.explanation}</div>
      <div className="rec">{insight.recommendation}</div>
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
