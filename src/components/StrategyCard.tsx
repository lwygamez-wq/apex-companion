import { useEffect, useState } from "react";
import { api } from "../lib/api";
import { lapTime } from "../lib/format";
import { useAsync } from "../lib/hooks";
import type { CircuitOverview, RaceLength } from "../lib/types";

const l1 = (v: number) => v.toFixed(1).replace(".", ",");

/** Une ligne « carburant pour finir » ; ouverte : format, marge, et le plan d'arrêts s'il en faut. Préparer une course : carburant au départ, arrêts et fenêtre, d'après ta consommation réelle ici. */
export function StrategyCard({ circuit }: { circuit: CircuitOverview }) {
  const [length, setLength] = useState<RaceLength | null>(null);
  const [margin, setMargin] = useState(1);
  const [formation, setFormation] = useState(true);
  const view = useAsync(() => api.raceStrategy(circuit, length, margin, formation ? 0.5 : 0), [circuit.track, circuit.car, circuit.setup, circuit.track_config, length?.kind, length?.value, margin, formation]);
  // Nouveau circuit : on repart du format de la dernière course ici.
  useEffect(() => setLength(null), [circuit.track, circuit.car, circuit.setup]);
  const v = view.data;
  const len = length ?? v?.default_length ?? { kind: "minutes", value: 30 };
  const s = v?.strategy;

  return (
    <details className="card strategy">
      <summary className="strategy-line">
        <span className="muted">Carburant pour finir</span>
        {s ? (
          <>
            <b className="mono">{l1(s.stints[0]?.fuel_l ?? s.total_fuel_l)} L</b>
            <span className="muted">
              {s.laps} tours, marge comprise{s.stops > 0 ? ` · ${s.stops} arrêt${s.stops > 1 ? "s" : ""}` : ""}
            </span>
          </>
        ) : (
          <span className="muted">{view.error ? "pas encore de tours propres ici" : "…"}</span>
        )}
        <span className="muted strategy-more">régler</span>
      </summary>
      {v && <div className="muted" style={{ fontSize: 12, marginTop: 8 }}>D'après tes {v.laps_used} derniers tours propres.</div>}
      <div className="strategy-inputs">
        <div className="seg">
          <button className={len.kind === "minutes" ? "on" : ""} onClick={() => setLength({ kind: "minutes", value: len.kind === "minutes" ? len.value : 30 })}>Minutes</button>
          <button className={len.kind === "laps" ? "on" : ""} onClick={() => setLength({ kind: "laps", value: len.kind === "laps" ? len.value : s?.laps ?? 15 })}>Tours</button>
        </div>
        <input
          type="number"
          min={1}
          max={1440}
          value={len.value}
          aria-label={len.kind === "laps" ? "Nombre de tours" : "Durée en minutes"}
          onChange={(e) => {
            const n = Math.max(1, Number(e.target.value) || 1);
            setLength(len.kind === "laps" ? { kind: "laps", value: Math.round(n) } : { kind: "minutes", value: n });
          }}
        />
        <label className="muted">
          marge
          <select value={margin} onChange={(e) => setMargin(Number(e.target.value))} aria-label="Marge de sécurité">
            {[0, 0.5, 1, 2].map((m) => <option key={m} value={m}>{m === 0 ? "aucune" : `${String(m).replace(".", ",")} tour${m > 1 ? "s" : ""}`}</option>)}
          </select>
        </label>
        <label className="muted toolbar-check"><input type="checkbox" checked={formation} onChange={(e) => setFormation(e.target.checked)} /> tour de formation</label>
      </div>
      {view.error && <div className="muted">{view.error}</div>}
      {s && v && (
        <>
          {s.stops > 0 && <div className="strategy-summary">{s.summary}</div>}
          {s.stops > 0 && <div className="strategy-facts">
            <div><span>Tours</span><b className="mono">{s.laps}</b></div>
            <div><span>Carburant total</span><b className="mono">{l1(s.total_fuel_l)} L</b></div>
            <div><span>Départ</span><b className="mono">{l1(s.stints[0]?.fuel_l ?? 0)} L</b></div>
            <div><span>Arrêts</span><b className="mono">{s.stops}</b></div>
          </div>}
          {s.stints.length > 1 && (
            <div className="stints">
              {s.stints.map((st, i) => (
                <div key={i} className="stint">
                  <span className="muted">Relais {i + 1}</span>
                  <span>tours {st.from_lap} à {st.to_lap}</span>
                  <b className="mono">{i === 0 ? "départ" : "remplir"} {l1(st.fuel_l)} L</b>
                </div>
              ))}
            </div>
          )}
          <div className="muted" style={{ fontSize: 12, marginTop: 8 }}>
            {v.fuel_per_lap_l.toFixed(2).replace(".", ",")} L/tour en moyenne (max {v.fuel_per_lap_max_l.toFixed(2).replace(".", ",")}), rythme {lapTime(v.lap_time_s)}
            {v.tank_l != null ? `, réservoir ${Math.round(v.tank_l)} L` : ", réservoir inconnu"}. Au volant, « carburant » te dit en course s'il y a de quoi finir.
          </div>
        </>
      )}
    </details>
  );
}
