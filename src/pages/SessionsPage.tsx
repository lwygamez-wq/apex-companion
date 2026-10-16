import { save } from "@tauri-apps/plugin-dialog";
import { useMemo, useState } from "react";
import type { Route } from "../App";
import { ImportActions } from "../components/ImportActions";
import { api } from "../lib/api";
import { dateTime, lapTime, SESSION_KIND_FR } from "../lib/format";
import { useAsync } from "../lib/hooks";
import type { SessionKind, SessionSummary, SimId } from "../lib/types";

export function SessionsPage({ navigate }: { navigate: (r: Route) => void }) {
  const [sim, setSim] = useState<SimId | "">("");
  const [kind, setKind] = useState<SessionKind | "">("");
  const [track, setTrack] = useState("");
  const [car, setCar] = useState("");
  const [from, setFrom] = useState("");
  const all = useAsync(() => api.sessions(), []);
  const filter = useMemo(
    () => ({
      sim: sim || undefined,
      kind: kind || undefined,
      track: track || undefined,
      car: car || undefined,
      from_unix: from ? Math.floor(new Date(from).getTime() / 1000) : undefined,
    }),
    [sim, kind, track, car, from],
  );
  const list = useAsync(() => api.sessions(filter), [filter]);
  const uniq = (f: (s: SessionSummary) => string) => [...new Set((all.data ?? []).map(f))].sort();

  const [msg, setMsg] = useState<string | null>(null);
  const exportOne = async (s: SessionSummary) => {
    const date = new Date(s.started_at_unix * 1000).toISOString().slice(0, 10);
    const name = `apex-${s.track}-${date}`.replace(/[^\w.-]+/g, "_") + ".apex.json.gz";
    try {
      const path = await save({ defaultPath: name, filters: [{ name: "Export Apex", extensions: ["gz"] }] });
      if (!path) return;
      setMsg("Export en cours…");
      const r = await api.exportSession(s.id, path);
      setMsg(`Export prêt (${(r.bytes / 1e6).toFixed(1)} Mo, ${r.laps} tours) : glisse ce fichier dans ta conversation avec Claude.`);
    } catch (e) {
      setMsg(e instanceof Error ? e.message : String(e));
    }
  };
  const remove = async (id: number) => {
    await api.deleteSession(id);
    list.reload();
    all.reload();
  };

  return (
    <>
      <div className="page-title"><h1>Sessions</h1></div>
      <div className="card" style={{ marginBottom: 14 }}>
        <ImportActions onDone={() => (list.reload(), all.reload())} />
      </div>
      <div className="row" style={{ marginBottom: 12 }}>
        <select value={sim} onChange={(e) => setSim(e.target.value as SimId | "")}>
          <option value="">Tous les simulateurs</option>
          {uniq((s) => s.sim).map((s) => <option key={s} value={s}>{(all.data ?? []).find((x) => x.sim === s)?.sim_name}</option>)}
        </select>
        <select value={track} onChange={(e) => setTrack(e.target.value)}>
          <option value="">Tous les circuits</option>
          {uniq((s) => s.track).map((t) => <option key={t}>{t}</option>)}
        </select>
        <select value={car} onChange={(e) => setCar(e.target.value)}>
          <option value="">Toutes les voitures</option>
          {uniq((s) => s.car).map((c) => <option key={c}>{c}</option>)}
        </select>
        <select value={kind} onChange={(e) => setKind(e.target.value as SessionKind | "")}>
          <option value="">Tous les types</option>
          {Object.entries(SESSION_KIND_FR).map(([k, v]) => <option key={k} value={k}>{v}</option>)}
        </select>
        <label className="muted">depuis <input type="text" placeholder="AAAA-MM-JJ" value={from} onChange={(e) => setFrom(e.target.value)} style={{ width: 110 }} /></label>
      </div>
      {list.error && <div className="error">{list.error}</div>}
      {msg && <div className="warning">{msg}</div>}
      <div className="card" style={{ padding: 0 }}>
        <table>
          <thead>
            <tr>
              <th>Date</th><th>Simulateur</th><th>Circuit</th><th>Voiture</th><th>Type</th>
              <th className="num">Meilleur</th><th className="num">Moyenne</th><th className="num">σ</th>
              <th className="num">Tours</th><th className="num">Incidents</th><th className="num">Carb./tour</th><th>Météo</th><th />
            </tr>
          </thead>
          <tbody>
            {(list.data ?? []).map((s) => (
              <tr key={s.id} className="clickable" onClick={() => navigate({ page: "analysis", sessionId: s.id })}>
                <td>{dateTime(s.started_at_unix)}</td>
                <td>{s.sim_name}</td>
                <td>{s.track}{s.track_config ? <span className="muted">, {s.track_config}</span> : null}</td>
                <td>{s.car}</td>
                <td>{SESSION_KIND_FR[s.kind]}</td>
                <td className="num">{lapTime(s.best_lap_s)}</td>
                <td className="num">{lapTime(s.mean_clean_s)}</td>
                <td className="num">{s.stddev_clean_s != null ? s.stddev_clean_s.toFixed(3) : "—"}</td>
                <td className="num">{s.clean_lap_count}/{s.lap_count}</td>
                <td className="num">{s.incident_laps}</td>
                <td className="num">{s.fuel_per_lap_l != null ? `${s.fuel_per_lap_l.toFixed(2)} L` : "—"}</td>
                <td className="muted">
                  {s.weather.description ?? "—"}
                  {s.weather.track_temp_c != null ? `, piste ${s.weather.track_temp_c.toFixed(0)} °C` : ""}
                </td>
                <td onClick={(e) => e.stopPropagation()}>
                  <div className="row" style={{ flexWrap: "nowrap" }}>
                    <button className="btn" title="Fichier compact à envoyer pour analyse" onClick={() => void exportOne(s)}>Exporter</button>
                    <button className="btn danger" onClick={() => void remove(s.id)}>Supprimer</button>
                  </div>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        {list.data?.length === 0 && <div className="empty">Aucune session ne correspond aux filtres.</div>}
      </div>
    </>
  );
}
