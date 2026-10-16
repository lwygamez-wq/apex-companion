import { useEffect, useMemo, useState } from "react";
import type { Route } from "../App";
import { InsightCard } from "../components/InsightCard";
import { TelemetryCharts } from "../components/TelemetryCharts";
import { TrackMap } from "../components/TrackMap";
import { api } from "../lib/api";
import { dateTime, delta, LAP_KIND_FR, lapTime } from "../lib/format";
import { useAsync } from "../lib/hooks";
import type { LapRow, SessionDetail } from "../lib/types";

function lapFlags(l: LapRow): string {
  const f = [];
  if (l.flags.off_track) f.push("hors-piste");
  if (l.flags.incident) f.push("incident");
  if (l.flags.pit) f.push("stands");
  if (l.flags.yellow_flag) f.push("drapeau jaune");
  if (l.flags.discontinuity) f.push("discontinuité");
  return f.join(", ");
}

function defaultPair(d: SessionDetail): [number | null, number | null] {
  const ref = d.personal_reference_lap_id ?? d.best_lap_id;
  const clean = d.laps.filter((l) => l.clean && l.id !== ref);
  const cmp = clean[clean.length - 1]?.id ?? d.laps.find((l) => l.id !== ref && l.lap_time_s != null)?.id ?? null;
  return [ref, cmp];
}

export function LapAnalysisPage({ route, navigate }: { route: Extract<Route, { page: "analysis" }>; navigate: (r: Route) => void }) {
  const sessions = useAsync(() => api.sessions(), []);
  const sessionId = route.sessionId ?? sessions.data?.[0]?.id;
  const detail = useAsync(() => (sessionId != null ? api.session(sessionId) : Promise.resolve(null)), [sessionId]);
  const [refId, setRefId] = useState<number | null>(route.refLapId ?? null);
  const [cmpId, setCmpId] = useState<number | null>(route.cmpLapId ?? null);
  const [cursor, setCursor] = useState<number | null>(null);
  const [corner, setCorner] = useState<number | null>(null);
  const [showAll, setShowAll] = useState(false);
  const settings = useAsync(() => api.settings(), []);
  const beginner = settings.data?.beginner_mode ?? true;

  useEffect(() => {
    if (!detail.data) return;
    const [r, c] = defaultPair(detail.data);
    setRefId((x) => (x != null && detail.data!.laps.some((l) => l.id === x) ? x : r));
    setCmpId((x) => (x != null && detail.data!.laps.some((l) => l.id === x) ? x : c));
    setCorner(null);
  }, [detail.data]);

  const cmp = useAsync(
    () => (refId != null && cmpId != null && refId !== cmpId ? api.compare(refId, cmpId, 1) : Promise.resolve(null)),
    [refId, cmpId],
  );

  const d = detail.data;
  const c = cmp.data;
  const insights = useMemo(() => {
    if (!c) return [];
    const name = c.corners.find((k) => k.corner.id === corner)?.corner.name;
    return name ? c.insights.filter((i) => i.segment === name) : c.insights;
  }, [c, corner]);

  const markReference = async (lapId: number) => {
    await api.setReference(lapId);
    detail.reload();
  };

  const lapLabel = (l: LapRow) =>
    `Tour ${l.number} · ${lapTime(l.lap_time_s)}${l.id === d?.best_lap_id ? " · meilleur" : ""}${l.is_reference ? " · réf. perso" : ""}${l.clean ? "" : " · ⚠"}`;

  return (
    <>
      <div className="page-title">
        <h1>Analyse de tour</h1>
        <select value={sessionId ?? ""} onChange={(e) => navigate({ page: "analysis", sessionId: Number(e.target.value) })}>
          {(sessions.data ?? []).map((s) => (
            <option key={s.id} value={s.id}>{dateTime(s.started_at_unix)} — {s.track} — {s.car}</option>
          ))}
        </select>
      </div>
      {sessions.data?.length === 0 && <div className="card empty"><h2>Aucune session</h2><p>Importe une session depuis l'écran Sessions.</p></div>}
      {detail.error && <div className="error">{detail.error}</div>}
      {d && (
        <>
          <div className="grid cols-4" style={{ marginBottom: 14 }}>
            <div className="card"><h2>Session</h2><div style={{ fontWeight: 600 }}>{d.meta.track}</div><div className="kpi-sub">{d.meta.car}</div></div>
            <div className="card"><h2>Meilleur tour</h2><div className="kpi">{lapTime(d.laps.find((l) => l.id === d.best_lap_id)?.lap_time_s)}</div></div>
            <div className="card"><h2>Théorique</h2><div className="kpi">{lapTime(d.theoretical_best_s)}</div></div>
            <div className="card"><h2>Régularité</h2><div className="kpi">{d.consistency ? `σ ${d.consistency.stddev_s.toFixed(3)} s` : "—"}</div><div className="kpi-sub">{d.consistency ? `${d.consistency.clean_laps} tours propres` : ""}</div></div>
          </div>

          <div className="card" style={{ marginBottom: 14 }}>
            <div className="row" style={{ justifyContent: "space-between" }}>
              <div className="row">
                <span className="muted">Référence</span>
                <select value={refId ?? ""} onChange={(e) => setRefId(Number(e.target.value))}>
                  {d.personal_reference_lap_id != null && !d.laps.some((l) => l.id === d.personal_reference_lap_id) && (
                    <option value={d.personal_reference_lap_id}>Référence personnelle (autre session)</option>
                  )}
                  {d.laps.filter((l) => l.lap_time_s != null).map((l) => <option key={l.id} value={l.id}>{lapLabel(l)}</option>)}
                </select>
                <span className="muted">Tour analysé</span>
                <select value={cmpId ?? ""} onChange={(e) => setCmpId(Number(e.target.value))}>
                  {d.laps.filter((l) => l.lap_time_s != null).map((l) => <option key={l.id} value={l.id}>{lapLabel(l)}</option>)}
                </select>
              </div>
              {c && <span className={`mono ${(c.delta_s[c.delta_s.length - 1] ?? 0) > 0 ? "pos" : "neg"}`} style={{ fontSize: 18 }}>{delta(c.delta_s[c.delta_s.length - 1])} s</span>}
            </div>
          </div>

          {cmp.error && <div className="error">{cmp.error}</div>}
          {c?.warnings.map((w) => <div key={w} className="warning">⚠ {w}</div>)}
          {refId === cmpId && <div className="warning">Choisis deux tours différents.</div>}

          {c && (
            <div className="grid analysis">
              <div>
                <TelemetryCharts cmp={c} onCursor={setCursor} simple={beginner} />
                <div className="card" style={{ padding: 0, marginTop: 8 }}>
                  <table>
                    <thead>
                      <tr><th>Virage</th><th className="num">Δ temps</th><th className="num">Freinage réf./tour</th><th className="num">V. mini réf./tour</th><th className="num">V. sortie réf./tour</th><th className="num">Gaz réf./tour</th></tr>
                    </thead>
                    <tbody>
                      {c.corners.map((k) => (
                        <tr key={k.corner.id} className="clickable" onClick={() => setCorner(corner === k.corner.id ? null : k.corner.id)} style={corner === k.corner.id ? { background: "var(--panel-2)" } : undefined}>
                          <td className="mono">{k.corner.name}</td>
                          <td className={`num ${k.time_delta_s > 0.02 ? "pos" : k.time_delta_s < -0.02 ? "neg" : ""}`}>{delta(k.time_delta_s, 2)}</td>
                          <td className="num">{k.reference.brake_on_m?.toFixed(0) ?? "—"} / {k.compared.brake_on_m?.toFixed(0) ?? "—"} m</td>
                          <td className="num">{k.reference.min_speed_kmh.toFixed(0)} / {k.compared.min_speed_kmh.toFixed(0)}</td>
                          <td className="num">{k.reference.exit_speed_kmh.toFixed(0)} / {k.compared.exit_speed_kmh.toFixed(0)}</td>
                          <td className="num">{k.reference.throttle_on_m?.toFixed(0) ?? "—"} / {k.compared.throttle_on_m?.toFixed(0) ?? "—"} m</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                  <div className="muted" style={{ padding: "8px 10px", fontSize: 12 }}>
                    Numérotation des virages détectée automatiquement (V1, V2…) : elle peut différer de la numérotation officielle du circuit.
                  </div>
                </div>
              </div>
              <div>
                <TrackMap cmp={c} cursor={cursor} selected={corner} onSelectCorner={(id) => setCorner(corner === id ? null : id)} />
                <h2 style={{ fontSize: 13, color: "var(--muted)", textTransform: "uppercase", letterSpacing: "0.08em", margin: "16px 0 8px" }}>
                  {corner != null ? `Conseils — ${c.corners.find((k) => k.corner.id === corner)?.corner.name}` : beginner ? "Tes 3 priorités" : "Conseils prioritaires"}
                </h2>
                {insights.length === 0 && <div className="card muted">Aucun écart significatif détecté{corner != null ? " sur ce virage" : ""}.</div>}
                {(beginner && corner == null && !showAll ? insights.slice(0, 3) : insights).map((i) => <InsightCard key={i.id} insight={i} />)}
                {beginner && corner == null && insights.length > 3 && (
                  <button className="btn" onClick={() => setShowAll((v) => !v)}>
                    {showAll ? "Revenir aux 3 priorités" : `Voir les ${insights.length - 3} autres conseils`}
                  </button>
                )}
                {beginner && corner == null && insights.length > 0 && (
                  <div className="muted" style={{ fontSize: 12, marginTop: 8 }}>
                    Travaille un seul conseil par session : c'est plus efficace que de tout corriger en même temps.
                  </div>
                )}
              </div>
            </div>
          )}

          <div className="card" style={{ padding: 0, marginTop: 14 }}>
            <table>
              <thead><tr><th>Tour</th><th>Type</th><th className="num">Temps</th><th>Validité</th><th>Perturbations</th><th className="num">Carburant</th><th /></tr></thead>
              <tbody>
                {d.laps.map((l) => (
                  <tr key={l.id}>
                    <td className="mono">{l.number}{l.id === d.best_lap_id && <span className="chip good" style={{ marginLeft: 6 }}>meilleur</span>}</td>
                    <td>{LAP_KIND_FR[l.kind]}</td>
                    <td className="num">{lapTime(l.lap_time_s)}</td>
                    <td>
                      <span className={`chip ${l.validity === "valid" ? "good" : l.validity === "invalid" ? "bad" : ""}`}>{l.validity === "valid" ? "valide" : l.validity === "invalid" ? "invalide" : "inconnue"}</span>
                      {l.validity_source === "derived" && <span className="muted" title="Déduite de la télémétrie (hors-piste, incidents), non rapportée par le simulateur."> (estimée)</span>}
                    </td>
                    <td className="muted">{lapFlags(l)}</td>
                    <td className="num">{l.fuel_used_l != null ? `${l.fuel_used_l.toFixed(2)} L` : "—"}</td>
                    <td>
                      {l.is_reference ? <span className="chip warn">référence perso</span> : l.lap_time_s != null && (
                        <button className="btn" onClick={() => void markReference(l.id)} title={l.clean ? "" : "Tour perturbé : référence déconseillée"}>Définir comme référence</button>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </>
      )}
    </>
  );
}
