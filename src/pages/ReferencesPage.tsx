import { useMemo } from "react";
import { api } from "../lib/api";
import { useAsync } from "../lib/hooks";
import { lapTime } from "../lib/format";
import { carLabel } from "./SetupsPage";
import type { ReferenceEntry } from "../lib/types";
import type { Route } from "../App";

const COND: Record<ReferenceEntry["conditions"], string> = { sec: "Sec", humide: "Humide", inconnu: "Conditions inconnues" };
const SETUP: Record<string, string> = { fixed: "Fixed", open: "Open" };

const day = (unix: number) => (unix > 0 ? new Date(unix * 1000).toLocaleDateString("fr-FR", { day: "numeric", month: "short", year: "numeric" }) : "—");

/** Gain d'une référence sur la précédente, en secondes (`null` sans version précédente). */
export function gainOverPrevious(r: ReferenceEntry): number | null {
  const prev = r.history[0];
  return prev ? prev.lap_time_s - r.lap_time_s : null;
}

/** Onglet Références : la bibliothèque élue automatiquement, une ligne par circuit, voiture, série et conditions. */
export function ReferencesPage({ navigate }: { navigate: (r: Route) => void }) {
  const refs = useAsync(() => api.references(), []);
  const byTrack = useMemo(() => {
    const m = new Map<string, ReferenceEntry[]>();
    for (const r of refs.data ?? []) {
      const k = r.track_config ? `${r.track} · ${r.track_config}` : r.track;
      m.set(k, [...(m.get(k) ?? []), r]);
    }
    return [...m.entries()];
  }, [refs.data]);

  return (
    <div className="page references-page">
      <div className="page-title">
        <h1>Références</h1>
      </div>
      <div className="card">
        <p className="muted">
          Pour chaque circuit et chaque voiture, Apex garde ton meilleur tour propre : tour chronométré, valide, sans sortie, sans drapeau ni passage au stand, télémétrie complète. Sec et pluie sont séparés, comme fixed et open. Elle sert de référence à l'analyse et au coach, sauf si tu en as choisi une toi-même.
        </p>
        {refs.error && <div className="fc-warn">{refs.error}</div>}
        {refs.data && refs.data.length === 0 && <div className="muted">Aucune référence pour l'instant : elles apparaissent dès qu'une séance avec un tour propre complet est importée.</div>}
        {byTrack.map(([track, list]) => (
          <div key={track} className="su-group ref-group">
            <h3>{track}</h3>
            <table className="su-table">
              <thead>
                <tr>
                  <th>Voiture</th>
                  <th>Série</th>
                  <th>Piste</th>
                  <th>Temps</th>
                  <th>Gain</th>
                  <th>Séance</th>
                  <th></th>
                </tr>
              </thead>
              <tbody>
                {list.map((r) => {
                  const g = gainOverPrevious(r);
                  return (
                    <tr key={r.id}>
                      <td>
                        {r.sim === "lmu" ? carLabel(r.car) : r.car}
                        {r.car_class && <span className="muted"> · {r.car_class}</span>}
                      </td>
                      <td>{r.setup ? SETUP[r.setup] : "—"}</td>
                      <td>{COND[r.conditions]}</td>
                      <td className="num">{lapTime(r.lap_time_s)}</td>
                      <td className="num">{g == null ? "première" : `−${g.toFixed(3).replace(".", ",")} s sur la précédente`}</td>
                      <td className="muted">{day(r.session_started_at)}, tour {r.lap_number}</td>
                      <td>
                        <button className="btn ghost" onClick={() => navigate({ page: "analysis", sessionId: r.session_id, refLapId: r.lap_id ?? undefined })}>Ouvrir</button>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        ))}
        <p className="muted su-note">Référence de la catégorie (meilleur tour d'une autre voiture) : non affichée tant que la sonde n'a pas été validée sur une vraie course.</p>
      </div>
    </div>
  );
}
