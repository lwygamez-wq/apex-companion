import { useEffect, useMemo, useState } from "react";
import type { Route } from "../App";
import { InsightCard } from "../components/InsightCard";
import { TelemetryCharts } from "../components/TelemetryCharts";
import { OffTrackCard } from "../components/OffTrackCard";
import { RiskCard } from "../components/RiskCard";
import { RaceCard } from "../components/RaceCard";
import { ReplayPanel } from "../components/ReplayPanel";
import { TrackMap, type MapSize } from "../components/TrackMap";
import { ReplayStudio } from "../components/ReplayStudio";
import { api } from "../lib/api";
import { dateTime, LAP_KIND_FR, lapTime, SESSION_KIND_FR } from "../lib/format";
import { useAsync } from "../lib/hooks";
import { ReplayStore, replayBounds } from "../lib/replay";
import type { LapRow, SessionDetail, SessionProfile } from "../lib/types";
import { SetupChip } from "../components/SetupChip";
import { CornerList } from "../components/CornerList";
import { finalGap, fmtGap, type Axis, type GapUnit } from "../lib/compareView";

function useWindowHeight(): number {
  const [h, setH] = useState(() => window.innerHeight);
  useEffect(() => {
    const f = () => setH(window.innerHeight);
    window.addEventListener("resize", f);
    return () => window.removeEventListener("resize", f);
  }, []);
  return h;
}

/** Réglage d'affichage gardé d'une visite à l'autre (navigateur uniquement, facultatif). */
function usePref<T>(key: string, init: T): [T, (v: T | ((p: T) => T)) => void] {
  const [v, setV] = useState<T>(() => {
    try {
      const s = localStorage.getItem(key);
      return s == null ? init : (JSON.parse(s) as T);
    } catch {
      return init;
    }
  });
  useEffect(() => {
    try {
      localStorage.setItem(key, JSON.stringify(v));
    } catch {
      /* stockage indisponible : réglage non gardé */
    }
  }, [key, v]);
  return [v, setV];
}

function Segmented<T extends string>({ label, value, options, onChange, title }: { label: string; value: T; options: [T, string][]; onChange: (v: T) => void; title?: string }) {
  return (
    <div className="seg" title={title}>
      <span className="seg-label">{label}</span>
      {options.map(([k, l]) => (
        <button key={k} className={value === k ? "on" : ""} onClick={() => onChange(k)}>{l}</button>
      ))}
    </div>
  );
}

type Tab = "advice" | "telemetry" | "corners" | "race" | "laps";

/** Fiche voiture en quelques repères lisibles. */
function carFacts(p: SessionProfile): string[] {
  const c = p.car;
  const tank = c.fuel_max_l != null ? Math.round(c.fuel_max_l * (c.max_fuel_pct ?? 1)) : null;
  return [
    c.class,
    tank != null && !c.electric ? `réservoir ${tank} L` : null,
    c.gears ? `${c.gears} rapports` : null,
    c.shift_rpm ? `passage ${Math.round(c.shift_rpm)} tr/min` : null,
    c.redline_rpm ? `rupteur ${Math.round(c.redline_rpm)}` : null,
  ].filter((x): x is string => !!x);
}

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
  const ref = d.analysis_reference_lap_id ?? d.personal_reference_lap_id ?? d.best_lap_id;
  const clean = d.laps.filter((l) => l.clean && l.id !== ref);
  const flying = d.laps.filter((l) => l.kind === "flying" && l.lap_time_s != null && l.id !== ref);
  const cmp = clean[clean.length - 1]?.id ?? flying[flying.length - 1]?.id ?? d.laps.find((l) => l.id !== ref && l.lap_time_s != null)?.id ?? null;
  return [ref, cmp];
}

export function LapAnalysisPage({ route, navigate }: { route: Extract<Route, { page: "analysis" }>; navigate: (r: Route) => void }) {
  const sessions = useAsync(() => api.sessions(), []);
  // Par défaut : la session la plus récente qui a des tours chronométrés (pas une sortie des stands).
  const sessionId = route.sessionId ?? (sessions.data?.find((s) => s.best_lap_s != null || s.lap_count > 3) ?? sessions.data?.[0])?.id;
  const [tab, setTab] = usePref<Tab>("apex.analysisTab", "advice");
  const detail = useAsync(() => (sessionId != null ? api.session(sessionId) : Promise.resolve(null)), [sessionId]);
  const [refId, setRefId] = useState<number | null>(route.refLapId ?? null);
  const [cmpId, setCmpId] = useState<number | null>(route.cmpLapId ?? null);
  const [corner, setCorner] = useState<number | null>(null);
  const [showAll, setShowAll] = useState(false);
  const [mapSize, setMapSize] = usePref<MapSize>("apex.mapSize", "normal");
  const bigMap = mapSize !== "normal";
  const winH = useWindowHeight();
  useEffect(() => {
    if (mapSize === "half") requestAnimationFrame(() => document.querySelector(".replay-big")?.scrollIntoView({ behavior: "smooth", block: "start" }));
  }, [mapSize]);

  const [axis, setAxis] = usePref<Axis>("apex.axis", "distance");
  const [gapUnit, setGapUnit] = usePref<GapUnit>("apex.gap", "s");
  const [more, setMore] = usePref<boolean>("apex.moreCharts", false);
  const saveName = async (apex: number, name: string) => {
    if (!detail.data) return;
    await api.setCornerName(detail.data.id, apex, name);
    cmp.reload();
  };
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
  const overlay = useAsync(() => (d && refId != null ? api.trackOverlay(d.id, refId).catch(() => null) : Promise.resolve(null)), [d?.id, refId]);
  const store = useMemo(() => (c ? new ReplayStore(...replayBounds(c)) : null), [c]);
  useEffect(() => () => store?.dispose(), [store]);
  const onChartCursor = (t: number | null) => {
    if (!store || t == null || store.get().playing) return;
    store.seek(t, true);
  };
  const endGap = useMemo(() => (c ? finalGap(c, gapUnit) : null), [c, gapUnit]);
  const insights = useMemo(() => {
    if (!c) return [];
    const name = c.corners.find((k) => k.corner.id === corner)?.corner.name;
    return name ? c.insights.filter((i) => i.segment === name) : c.insights;
  }, [c, corner]);

  const [g61Msg, setG61Msg] = useState<string | null>(null);
  const findG61 = async () => {
    if (!d) return;
    setG61Msg("Recherche sur Garage 61…");
    try {
      setG61Msg(await api.findGarage61Reference(d.id));
      detail.reload();
    } catch (e) {
      setG61Msg(e instanceof Error ? e.message : String(e));
    }
  };
  const markReference = async (lapId: number) => {
    await api.setReference(lapId);
    detail.reload();
  };

  const lapLabel = (l: LapRow) =>
    `Tour ${l.number}, ${lapTime(l.lap_time_s)}${l.id === d?.best_lap_id ? ", meilleur" : ""}${l.is_reference ? ", réf. perso" : ""}${l.clean ? "" : " (perturbé)"}`;
  const timed = d ? d.laps.filter((l) => l.lap_time_s != null) : [];
  const tabs: [Tab, string][] = [["advice", "Conseils"], ["telemetry", "Télémétrie"], ["corners", "Virages"], ...(d?.race ? ([["race", "Course"]] as [Tab, string][]) : []), ["laps", "Tours"]];
  const current: Tab = tabs.some((t) => t[0] === tab) ? tab : "advice";
  const best = d ? d.laps.find((l) => l.id === (d.best_lap_id ?? d.fastest_lap_id))?.lap_time_s : null;

  return (
    <>
      <div className="page-title">
        <h1>Analyse</h1>
        <select className="session-pick" value={sessionId ?? ""} onChange={(e) => navigate({ page: "analysis", sessionId: Number(e.target.value) })} aria-label="Session">
          {(sessions.data ?? []).map((s) => (
            <option key={s.id} value={s.id}>{dateTime(s.started_at_unix)} · {s.track} · {SESSION_KIND_FR[s.kind]}{s.best_lap_s != null ? ` · ${lapTime(s.best_lap_s)}` : ""}</option>
          ))}
        </select>
      </div>
      {sessions.data?.length === 0 && <div className="card empty"><h2>Aucune session</h2><p>Importe une session depuis l'écran Sessions.</p></div>}
      {detail.error && <div className="error">{detail.error}</div>}
      {d && (
        <>
          <div className="session-strip">
            <div className="ss-where">
              <b>{d.meta.track}</b>
              <span className="muted">
                {d.meta.car} · {SESSION_KIND_FR[d.meta.kind]} <SetupChip setup={d.meta.setup} />
              </span>
              {d.profile && (
                <span className="ss-facts" title={[d.profile.track.city && `${d.profile.track.city}, ${d.profile.track.country}`, d.profile.track.turns && `${d.profile.track.turns} virages`, d.bank && `Base circuits : ${d.bank.venue}`].filter(Boolean).join(" · ")}>
                  {carFacts(d.profile).slice(0, 3).join(" · ")}
                </span>
              )}
            </div>
            <div className="ss-kpi"><span>{d.best_lap_id != null || d.fastest_lap_id == null ? "Meilleur tour" : "Plus rapide (invalide)"}</span><b className="mono best">{lapTime(best)}</b></div>
            <div className="ss-kpi"><span>Théorique</span><b className="mono best">{lapTime(d.theoretical_best_s)}</b></div>
            <div className="ss-kpi"><span>Régularité</span><b className="mono">{d.consistency ? `± ${d.consistency.stddev_s.toFixed(2)} s` : "—"}</b></div>
            <div className="ss-kpi"><span>Tours propres</span><b className="mono">{d.laps.filter((l) => l.clean).length}/{timed.length}</b></div>
          </div>

          {d.untyped_references.length > 0 && (
            <div className="warning untyped">
              <div>
                Ta session est en série <b>{d.meta.setup === "fixed" ? "Fixed" : "Open"}</b>. Indique la série de ces références (elles ne sont pas utilisées en attendant) :
              </div>
              {d.untyped_references.map((u) => (
                <div key={u.session_id} className="row" style={{ gap: 8, marginTop: 6 }}>
                  <b>{u.driver}</b> <span className="mono">{lapTime(u.lap_time_s)}</span>
                  <button className="btn small" onClick={() => void api.setSessionSetup(u.session_id, "fixed").then(() => (detail.reload(), overlay.reload()))}>Fixed</button>
                  <button className="btn small" onClick={() => void api.setSessionSetup(u.session_id, "open").then(() => (detail.reload(), overlay.reload()))}>Open</button>
                </div>
              ))}
            </div>
          )}

          {timed.length < 2 && !(d.personal_reference_lap_id != null && timed.length === 1) ? (
            <div className="card empty"><h2>Pas assez de tours</h2><p>Cette session n'a {timed.length === 0 ? "aucun tour chronométré" : "qu'un tour chronométré"} : choisis une autre session en haut à droite.</p></div>
          ) : (
            <>
              <div className="duel">
                <div className="duel-lap ref">
                  <span className="duel-tag"><i className="swatch ref" />Référence</span>
                  <select value={refId ?? ""} onChange={(e) => setRefId(Number(e.target.value))} aria-label="Tour de référence">
                    {d.personal_reference_lap_id != null && !d.laps.some((l) => l.id === d.personal_reference_lap_id) && (
                      <option value={d.personal_reference_lap_id}>{d.reference_label ?? "Référence personnelle (autre session)"}</option>
                    )}
                    {timed.map((l) => <option key={l.id} value={l.id}>{lapLabel(l)}</option>)}
                  </select>
                  <div className="duel-time mono">{lapTime(c?.reference.lap_time_s)}</div>
                  <div className="duel-who">
                    {c ? c.reference.driver ?? `Tour ${c.reference.number}` : ""}
                    {settings.data?.garage61_token && d.meta.sim === "iracing" && (
                      <button className="link" onClick={() => void findG61()} title="Cherche le tour le plus rapide des pilotes que tu suis sur Garage 61">chercher sur Garage 61</button>
                    )}
                  </div>
                  {g61Msg && <div className="muted" style={{ fontSize: 12 }}>{g61Msg}</div>}
                </div>
                <div className="duel-gap">
                  <button className="btn ghost small" title="Inverser référence et tour analysé" onClick={() => { setRefId(cmpId); setCmpId(refId); }} disabled={!d.laps.some((l) => l.id === refId)}>⇄</button>
                  {c && <div className={`duel-delta mono ${(endGap ?? 0) > 0 ? "pos" : "neg"}`}>{fmtGap(endGap, gapUnit, gapUnit === "s" ? 3 : 0)}</div>}
                  {c && store && <button className="btn primary" onClick={() => setMapSize("full")} title="Carte et télémétrie synchronisées, en plein écran">▶ Relecture</button>}
                </div>
                <div className="duel-lap cmp">
                  <span className="duel-tag"><i className="swatch cmp" />Ton tour</span>
                  <select value={cmpId ?? ""} onChange={(e) => setCmpId(Number(e.target.value))} aria-label="Tour analysé">
                    {timed.map((l) => <option key={l.id} value={l.id}>{lapLabel(l)}</option>)}
                  </select>
                  <div className="duel-time mono">{lapTime(c?.compared.lap_time_s)}</div>
                  <div className="duel-who">{c ? `Tour ${c.compared.number}${c.compared.clean ? "" : ", perturbé"}` : ""}</div>
                </div>
              </div>

              {cmp.error && <div className="error">{cmp.error}</div>}
              {refId === cmpId && <div className="warning">Choisis deux tours différents.</div>}
              {c && c.warnings.length > 0 && (
                <details className="notes">
                  <summary>⚠ {c.warnings[0]}{c.warnings.length > 1 ? ` (+${c.warnings.length - 1})` : ""}</summary>
                  {c.warnings.slice(1).map((w) => <div key={w}>{w}</div>)}
                </details>
              )}

              <div className="tabs" role="tablist">
                {tabs.map(([k, l]) => (
                  <button key={k} role="tab" aria-selected={current === k} className={current === k ? "on" : ""} onClick={() => setTab(k)}>{l}</button>
                ))}
              </div>

              {c && store && current === "advice" && (
                <div className="insights-strip">
                  {corner != null && (
                    <div className="row" style={{ marginBottom: 8 }}>
                      <span className="muted">Conseils pour <b>{c.corners.find((k) => k.corner.id === corner)?.corner.name}</b></span>
                      <button className="btn ghost small" onClick={() => setCorner(null)}>tous les virages</button>
                    </div>
                  )}
                  {insights.length === 0 && <div className="card muted">Aucun écart significatif détecté{corner != null ? " sur ce virage" : ""}.</div>}
                  <div className="insight-grid">
                    {(beginner && corner == null && !showAll ? insights.slice(0, 3) : insights).map((i, k) => <InsightCard key={i.id} insight={i} sessionId={d.id} rank={k + 1} />)}
                  </div>
                  {beginner && corner == null && insights.length > 3 && (
                    <button className="btn ghost" onClick={() => setShowAll((v) => !v)}>
                      {showAll ? "Revenir aux 3 priorités" : `Voir les ${insights.length - 3} autres conseils`}
                    </button>
                  )}
                </div>
              )}

              {c && store && current === "telemetry" && (
                <>
                  {mapSize === "half" && (
                    <div className="replay-big">
                      <TrackMap overlay={overlay.data} cmp={c} store={store} selected={corner} onSelectCorner={(id) => setCorner(corner === id ? null : id)} height={Math.max(360, Math.round(winH * 0.5))} size={mapSize} onSize={setMapSize} />
                      <ReplayPanel cmp={c} store={store} />
                    </div>
                  )}
                  <div className={bigMap ? "" : "grid analysis"}>
                    <div className="card charts-card">
                      <div className="charts-bar">
                        <div className="legend">
                          <span><i className="swatch ref" />{c.reference.driver ?? `Tour ${c.reference.number}`}</span>
                          <span><i className="swatch cmp" />Toi, tour {c.compared.number}</span>
                        </div>
                        <div className="row" style={{ gap: 10 }}>
                          <Segmented label="Axe" value={axis} onChange={setAxis} options={[["distance", "Distance"], ["time", "Temps"]]} title="Distance : la référence est calée au même endroit de la piste. Temps : chaque tour à son chrono." />
                          <Segmented label="Écart" value={gapUnit} onChange={setGapUnit} options={[["s", "s"], ["m", "m"]]} title="Écart en secondes ou en mètres de retard sur la référence" />
                          <button className={`btn ghost small ${more ? "on" : ""}`} onClick={() => setMore((v) => !v)}>{more ? "Moins de courbes" : "Plus de courbes"}</button>
                        </div>
                      </div>
                      <TelemetryCharts cmp={c} onCursorTime={onChartCursor} axis={axis} gapUnit={gapUnit} more={more} selected={corner} store={store ?? undefined} />
                    </div>
                    {!bigMap && (
                      <div className="analysis-side">
                        <TrackMap overlay={overlay.data} cmp={c} store={store} selected={corner} onSelectCorner={(id) => setCorner(corner === id ? null : id)} size={mapSize} onSize={setMapSize} />
                        <ReplayPanel cmp={c} store={store} />
                      </div>
                    )}
                  </div>
                </>
              )}

              {c && store && current === "corners" && (
                <>
                  <CornerList corners={c.corners} selected={corner} onSelect={(id) => setCorner(id)} onRename={(apex, name) => void saveName(apex, name)} />
                  <div className="grid cols-2" style={{ marginTop: 14, alignItems: "start" }}>
                    <RiskCard risk={d.risk} headline={d.risk_headline} limit={4} causes={d.regularity?.corners} onLap={(n) => { const l = d.laps.find((x) => x.number === n && x.lap_time_s != null); if (l) { setCmpId(l.id); setTab("telemetry"); } }} onCorner={(apex) => { const k = c.corners.find((x) => Math.abs(x.corner.apex_m - apex) < 60); if (k) setCorner(corner === k.corner.id ? null : k.corner.id); }} />
                    <OffTrackCard zones={d.off_tracks} />
                  </div>
                </>
              )}
            </>
          )}

          {current === "race" && d.race && <RaceCard r={d.race} laps={d.laps} onLap={(id) => (setCmpId(id), setTab("telemetry"))} />}

          {current === "laps" && (
            <div className="card" style={{ padding: 0 }}>
              <table>
                <thead><tr><th>Tour</th><th className="num">Temps</th><th>État</th><th>Détail</th><th className="num">Carburant</th><th /></tr></thead>
                <tbody>
                  {d.laps.map((l) => (
                    <tr key={l.id} className={l.lap_time_s == null ? "dim" : ""}>
                      <td className="mono">{l.number}{l.id === d.best_lap_id && <span className="chip best" style={{ marginLeft: 6 }}>meilleur</span>}</td>
                      <td className="num">{lapTime(l.lap_time_s)}</td>
                      <td>
                        {l.kind !== "flying" ? <span className="muted">{LAP_KIND_FR[l.kind]}</span> : <span className={`chip ${l.clean ? "good" : "bad"}`}>{l.clean ? "propre" : "perturbé"}</span>}
                      </td>
                      <td className="muted">{lapFlags(l)}</td>
                      <td className="num">{l.fuel_used_l != null ? `${l.fuel_used_l.toFixed(2)} L` : "—"}</td>
                      <td>
                        {l.is_reference ? <span className="chip warn">référence perso</span> : l.lap_time_s != null && l.kind === "flying" && (
                          <button className="btn ghost small" onClick={() => void markReference(l.id)} title={l.clean ? "" : "Tour perturbé : référence déconseillée"}>Prendre comme référence</button>
                        )}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
          {mapSize === "full" && c && store && <ReplayStudio overlay={overlay.data} cmp={c} store={store} title={`${d.meta.track} · ${d.meta.car}`} onClose={() => setMapSize("normal")} />}
        </>
      )}
    </>
  );
}
