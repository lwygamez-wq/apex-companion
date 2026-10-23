import { useState } from "react";
import type { CornerRisk, RiskVerdict } from "../lib/types";

const VERDICT: Record<RiskVerdict, { label: string; cls: string }> = {
  secure: { label: "À sécuriser", cls: "bad" },
  cuts: { label: "Coupe de piste", cls: "bad" },
  push: { label: "Peut attaquer", cls: "good" },
  fine: { label: "OK", cls: "" },
  insufficient: { label: "Pas assez de passages", cls: "" },
};

/**
 * Carte de risque : gain disponible contre probabilité de sortie, virage par virage,
 * sur toutes les sessions du même circuit et de la même voiture.
 */
export function RiskCard({ risk, headline, onCorner, compact = false, limit }: { risk: CornerRisk[]; headline: string | null; onCorner?: (apexM: number) => void; compact?: boolean; limit?: number }) {
  const [open, setOpen] = useState<string | null>(null);
  const [all, setAll] = useState(false);
  const judged = risk.filter((r) => r.verdict !== "insufficient");
  if (risk.length === 0) return null;
  const sessions = Math.max(0, ...risk.map((r) => r.sessions));
  const passes = Math.max(0, ...risk.map((r) => r.passes));
  return (
    <div className="card risk">
      <div className="row" style={{ justifyContent: "space-between", alignItems: "baseline" }}>
        <h2>Carte de risque</h2>
        <span className="muted" style={{ fontSize: 12 }}>{passes} tours lancés · {sessions} session{sessions > 1 ? "s" : ""}</span>
      </div>
      {headline && <div className="risk-headline">{headline}</div>}
      {judged.length === 0 && <div className="muted">Il faut au moins 4 tours lancés sur ce circuit avec cette voiture pour juger le risque par virage.</div>}
      <div className="risk-grid">
        {(limit && !all ? judged.slice(0, limit) : judged).map((r) => {
          const v = VERDICT[r.verdict];
          const isOpen = open === r.name;
          return (
            <div key={r.name} className={`risk-row ${r.verdict}`} onClick={() => { setOpen(isOpen ? null : r.name); onCorner?.(r.apex_m); }}>
              <div className="risk-main">
                <b>{r.name}</b>
                <span className={`chip ${v.cls}`}>{v.label}</span>
              </div>
              {!compact && (
                <div className="risk-stats">
                  <span title="Passages avec une sortie de piste / passages évalués"><i className="risk-bar"><i style={{ width: `${Math.round(r.off_rate * 100)}%` }} /></i> {r.offs}/{r.passes} sorties</span>
                  <span title="Ce que coûte une sortie, en moyenne, dans ce virage">{r.off_cost_s == null ? "" : `sortie : ${r.off_cost_s >= 0 ? "+" : "−"}${Math.abs(r.off_cost_s).toFixed(1)} s`}</span>
                  <span title="Écart entre ton passage moyen propre et ton meilleur passage propre">{r.gain_available_s == null ? "" : `à prendre : ${r.gain_available_s.toFixed(2)} s`}</span>
                </div>
              )}
              {(isOpen || compact) && <div className="risk-msg">{r.message}</div>}
            </div>
          );
        })}
      </div>
      {limit != null && judged.length > limit && (
        <button className="btn ghost small" style={{ marginTop: 8 }} onClick={() => setAll((v) => !v)}>{all ? "Réduire" : `Voir les ${judged.length - limit} autres virages`}</button>
      )}
      <div className="muted" style={{ fontSize: 12, marginTop: 8 }}>
        « À prendre » = répéter ton propre meilleur passage propre, pas celui d'un autre pilote. Une sortie coûte aussi un tour invalide et un incident, non comptés ici.
      </div>
    </div>
  );
}
