import { useMemo, useState } from "react";
import type { Route } from "../App";
import { api } from "../lib/api";
import { dateTime, delta, lapTime } from "../lib/format";
import { useAsync } from "../lib/hooks";
import type { SessionSummary } from "../lib/types";

/** Progression sur un même combo simulateur / circuit / voiture, session par session. */
export function ComparisonPage({ navigate }: { navigate: (r: Route) => void }) {
  const all = useAsync(() => api.sessions(), []);
  const combos = useMemo(() => {
    const m = new Map<string, SessionSummary[]>();
    for (const s of all.data ?? []) {
      const k = `${s.track}${s.track_config ? ` (${s.track_config})` : ""}, ${s.car}, ${s.sim_name}`;
      m.set(k, [...(m.get(k) ?? []), s]);
    }
    return [...m.entries()].sort((a, b) => b[1].length - a[1].length);
  }, [all.data]);
  const [key, setKey] = useState<string>("");
  const sel = combos.find((c) => c[0] === key) ?? combos[0];
  const rows = [...(sel?.[1] ?? [])].sort((a, b) => a.started_at_unix - b.started_at_unix);
  const first = rows.find((r) => r.best_lap_s != null)?.best_lap_s ?? null;

  return (
    <>
      <div className="page-title">
        <h1>Comparaison</h1>
        <select value={sel?.[0] ?? ""} onChange={(e) => setKey(e.target.value)}>
          {combos.map(([k, v]) => <option key={k} value={k}>{k} ({v.length} session(s))</option>)}
        </select>
      </div>
      <div className="warning">
        Progression entre sessions d'un même combo. La comparaison segment par segment entre deux tours (y compris d'une autre session via la référence personnelle) se fait dans « Analyse de tour ».
        La comparaison multi-relais et inter-voitures arrive en phase 3.
      </div>
      <div className="card" style={{ padding: 0 }}>
        <table>
          <thead><tr><th>Date</th><th className="num">Meilleur</th><th className="num">vs 1re session</th><th className="num">Moyenne propre</th><th className="num">σ</th><th className="num">Tours propres</th><th>Météo</th></tr></thead>
          <tbody>
            {rows.map((s) => (
              <tr key={s.id} className="clickable" onClick={() => navigate({ page: "analysis", sessionId: s.id })}>
                <td>{dateTime(s.started_at_unix)}</td>
                <td className="num">{lapTime(s.best_lap_s)}</td>
                <td className={`num ${s.best_lap_s != null && first != null && s.best_lap_s < first ? "neg" : ""}`}>{s.best_lap_s != null && first != null ? delta(s.best_lap_s - first) : "—"}</td>
                <td className="num">{lapTime(s.mean_clean_s)}</td>
                <td className="num">{s.stddev_clean_s?.toFixed(3) ?? "—"}</td>
                <td className="num">{s.clean_lap_count}/{s.lap_count}</td>
                <td className="muted">{s.weather.description ?? "inconnue"}{s.weather.track_temp_c != null ? `, ${s.weather.track_temp_c.toFixed(0)} °C` : ""}</td>
              </tr>
            ))}
          </tbody>
        </table>
        {rows.length === 0 && <div className="empty">Aucune session.</div>}
      </div>
    </>
  );
}
