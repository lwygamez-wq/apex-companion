import { useEffect, useMemo, useRef, useState } from "react";
import { InsightCard } from "../components/InsightCard";
import { OffTrackCard } from "../components/OffTrackCard";
import { RiskCard } from "../components/RiskCard";
import { ReplayPanel } from "../components/ReplayPanel";
import { TelemetryCharts } from "../components/TelemetryCharts";
import { TrackMap } from "../components/TrackMap";
import { dateTime, delta, lapTime } from "../lib/format";
import { ReplayStore, replayBounds } from "../lib/replay";
import type { Comparison, LapRow } from "../lib/types";
import { deleteSession, listSessions, loadSession, parseExport, saveSession, type MobileSession, type StoredSummary } from "./data";

type Screen = { s: "list" } | { s: "session"; key: string } | { s: "lap"; key: string; lapId: number };

function useBack(screen: Screen, setScreen: (s: Screen) => void) {
  // Le geste « retour » d'iOS / le bouton retour ramènent à l'écran précédent.
  const first = useRef(true);
  useEffect(() => {
    if (first.current) {
      first.current = false;
      history.replaceState(screen, "");
      return;
    }
    history.pushState(screen, "");
  }, [screen]);
  useEffect(() => {
    const on = (e: PopStateEvent) => setScreen((e.state as Screen | null) ?? { s: "list" });
    window.addEventListener("popstate", on);
    return () => window.removeEventListener("popstate", on);
  }, [setScreen]);
}

export function MobileApp() {
  const [screen, setScreen] = useState<Screen>({ s: "list" });
  const [list, setList] = useState<StoredSummary[] | null>(null);
  const [session, setSession] = useState<MobileSession | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const fileInput = useRef<HTMLInputElement>(null);
  useBack(screen, setScreen);

  const refresh = () => void listSessions().then(setList);
  useEffect(refresh, []);

  // Charge la session affichée (depuis le stockage du téléphone).
  useEffect(() => {
    if (screen.s === "list") return;
    if (session?.key === screen.key) return;
    setBusy("Ouverture de la session…");
    loadSession(screen.key)
      .then(setSession, (e: unknown) => {
        setErr(e instanceof Error ? e.message : String(e));
        setScreen({ s: "list" });
      })
      .finally(() => setBusy(null));
  }, [screen, session?.key]);

  const onFiles = async (picked: FileList | null) => {
    // Copie : la liste est vidée dès que le champ fichier est réinitialisé.
    const files = picked ? Array.from(picked) : [];
    if (!files.length) return;
    setErr(null);
    let last: MobileSession | null = null;
    for (const f of files) {
      setBusy(`Lecture de ${f.name}…`);
      try {
        const buf = await f.arrayBuffer();
        const s = await parseExport(buf);
        await saveSession(s, buf);
        last = s;
      } catch (e) {
        setErr(e instanceof Error ? e.message : String(e));
      }
    }
    setBusy(null);
    refresh();
    if (last && files.length === 1) {
      setSession(last);
      setScreen({ s: "session", key: last.key });
    }
  };

  const title = screen.s === "list" ? "Sessions" : session?.meta.track ?? "";

  return (
    <div className="m-app">
      <header className="m-head">
        {screen.s !== "list" ? (
          <button className="m-back" onClick={() => history.back()} aria-label="Retour">‹</button>
        ) : (
          <span className="m-logo">APEX</span>
        )}
        <div className="m-title">
          <b>{title}</b>
          {screen.s !== "list" && session && <span>{session.meta.car}</span>}
        </div>
        {screen.s === "list" && (
          <button className="btn primary" onClick={() => fileInput.current?.click()}>Ouvrir</button>
        )}
        <input ref={fileInput} type="file" multiple hidden onChange={(e) => { void onFiles(e.target.files); e.target.value = ""; }} />
      </header>
      <main className="m-main">
        {err && <div className="error" onClick={() => setErr(null)}>{err}</div>}
        {busy && <div className="m-busy">{busy}</div>}
        {screen.s === "list" && list && (
          <SessionList
            list={list}
            onOpen={(k) => setScreen({ s: "session", key: k })}
            onDelete={async (k) => { await deleteSession(k); refresh(); }}
            onPick={() => fileInput.current?.click()}
          />
        )}
        {screen.s === "session" && session?.key === screen.key && <SessionScreen s={session} onLap={(id) => setScreen({ s: "lap", key: session.key, lapId: id })} />}
        {screen.s === "lap" && session?.key === screen.key && <LapScreen s={session} lapId={screen.lapId} />}
      </main>
    </div>
  );
}

function SessionList({ list, onOpen, onDelete, onPick }: { list: StoredSummary[]; onOpen: (k: string) => void; onDelete: (k: string) => void; onPick: () => void }) {
  if (list.length === 0) {
    return (
      <div className="m-empty">
        <h2>Aucune session sur ce téléphone</h2>
        <p>Ouvre un export <b>.apex.json.gz</b> créé par Apex Companion sur ton PC.</p>
        <ol>
          <li>Sur le PC : Réglages → « Copie automatique pour le mobile » → choisis ton dossier <b>iCloud Drive</b>. Chaque nouvelle session y sera copiée.</li>
          <li>Ici : « Ouvrir » → Fichiers → iCloud Drive → choisis la session.</li>
        </ol>
        <button className="btn primary" onClick={onPick}>Ouvrir un export</button>
        <p className="muted" style={{ fontSize: 12 }}>Tout reste dans ton téléphone : rien n'est envoyé sur Internet.</p>
      </div>
    );
  }
  return (
    <div className="m-list">
      {list.map((s) => (
        <div key={s.key} className="m-card" onClick={() => onOpen(s.key)}>
          <div className="m-card-main">
            <b>{s.track}</b>
            <span className="muted">{s.car}</span>
            <span className="muted">{dateTime(s.started_at_unix)} · {s.lap_count} tours</span>
          </div>
          <div className="m-card-side">
            <span className="kpi best m-kpi">{lapTime(s.best_lap_s)}</span>
            <button
              className="m-del"
              onClick={(e) => {
                e.stopPropagation();
                if (confirm("Retirer cette session du téléphone ? (le fichier d'origine n'est pas supprimé)")) onDelete(s.key);
              }}
            >
              Retirer
            </button>
          </div>
        </div>
      ))}
    </div>
  );
}

function SessionScreen({ s, onLap }: { s: MobileSession; onLap: (id: number) => void }) {
  const byLap = useMemo(() => new Map(s.comparisons.map((c) => [c.compared.lap_id, c])), [s]);
  const best = s.laps.find((l) => l.id === s.best_lap_id);
  const ref = s.laps.find((l) => l.id === s.reference_lap_id);
  // Conseils les plus fréquents sur la session : ce qui revient le plus souvent.
  const top = useMemo(() => {
    const m = new Map<string, { n: number; gain: number }>();
    for (const c of s.comparisons) {
      for (const i of c.insights) {
        const e = m.get(i.title) ?? { n: 0, gain: 0 };
        e.n++;
        e.gain += i.est_gain_s ? (i.est_gain_s[0] + i.est_gain_s[1]) / 2 : 0;
        m.set(i.title, e);
      }
    }
    return [...m.entries()].sort((a, b) => b[1].n - a[1].n || b[1].gain - a[1].gain).slice(0, 3);
  }, [s]);
  return (
    <>
      <div className="m-kpis">
        {best || !s.fastest_lap_id ? (
          <div className="card"><h2>Meilleur</h2><div className="kpi best m-kpi">{lapTime(best?.lap_time_s)}</div></div>
        ) : (
          <div className="card"><h2>Plus rapide</h2><div className="kpi m-kpi">{lapTime(s.laps.find((l) => l.id === s.fastest_lap_id)?.lap_time_s)}</div><div className="kpi-sub">invalide</div></div>
        )}
        <div className="card"><h2>Théorique</h2><div className="kpi best m-kpi">{lapTime(s.theoretical_best_s)}</div></div>
        <div className="card"><h2>Régularité</h2><div className="kpi m-kpi">{s.consistency ? `σ ${s.consistency.stddev_s.toFixed(2)}` : "—"}</div></div>
      </div>
      <RiskCard risk={s.risk} headline={s.risk_headline} compact />
      <OffTrackCard zones={s.off_tracks} />
      {top.length > 0 && (
        <div className="card" style={{ marginBottom: 10 }}>
          <h2>Ce qui revient le plus sur la session</h2>
          {top.map(([t, e]) => (
            <div key={t} className="m-top"><b>{t}</b><span className="muted">{e.n} tour{e.n > 1 ? "s" : ""} sur {s.comparisons.length}</span></div>
          ))}
        </div>
      )}
      <h2 className="m-h2">Tours <span className="muted">· comparés {s.reference_label ? `à ${s.reference_label}` : `au tour ${ref?.number ?? "de référence"}`}</span></h2>
      <div className="m-laps">
        {s.laps.filter((l) => l.lap_time_s != null).map((l) => {
          const c = byLap.get(l.id);
          const d = c?.delta_s[c.delta_s.length - 1];
          const isRef = l.id === s.reference_lap_id;
          return (
            <button key={l.id} className="m-lap" disabled={!c} onClick={() => onLap(l.id)}>
              <span className="mono m-lapn">{l.number}</span>
              <span className="mono m-lapt">{lapTime(l.lap_time_s)}</span>
              <span className={`mono ${d == null ? "muted" : d > 0 ? "pos" : "neg"}`}>{isRef ? "réf." : d == null ? "" : `${delta(d)} s`}</span>
              <span className="m-lapf">{lapFlags(l, s)}</span>
              {c && <span className="m-chev">›</span>}
            </button>
          );
        })}
      </div>
    </>
  );
}

function lapFlags(l: LapRow, s: MobileSession) {
  return (
    <>
      {l.id === s.best_lap_id && <span className="chip best">meilleur</span>}
      {l.flags.off_track ? <span className="chip warn">hors piste</span> : !l.clean && <span className="chip warn">perturbé</span>}
      {l.kind !== "flying" && <span className="chip">{l.kind === "out_lap" ? "sortie" : l.kind === "in_lap" ? "entrée" : "interrompu"}</span>}
    </>
  );
}

type Tab = "conseils" | "relecture" | "courbes" | "virages";

function LapScreen({ s, lapId }: { s: MobileSession; lapId: number }) {
  const c = s.comparisons.find((x) => x.compared.lap_id === lapId) as Comparison;
  const [tab, setTab] = useState<Tab>("conseils");
  const store = useMemo(() => new ReplayStore(...replayBounds(c)), [c]);
  useEffect(() => () => store.dispose(), [store]);
  const total = c.delta_s[c.delta_s.length - 1] ?? 0;
  return (
    <>
      <div className="m-lapbar">
        <span>Tour <b>{c.compared.number}</b> · {lapTime(c.compared.lap_time_s)}</span>
        <span className={`mono ${total > 0 ? "pos" : "neg"}`}>{delta(total)} s vs {s.reference_label ? s.reference_label.split(" (")[0] : `tour ${c.reference.number}`}</span>
      </div>
      <div className="seg m-tabs">
        {(["conseils", "relecture", "courbes", "virages"] as const).map((t) => (
          <button key={t} className={tab === t ? "on" : ""} onClick={() => setTab(t)}>{t[0]!.toUpperCase() + t.slice(1)}</button>
        ))}
      </div>
      {c.warnings.map((w) => <div key={w} className="warning">⚠ {w}</div>)}
      {tab === "conseils" && (
        <div>
          {c.insights.length === 0 && <div className="card muted">Aucun écart significatif détecté sur ce tour.</div>}
          {c.insights.slice(0, 5).map((i) => <InsightCard key={i.id} insight={i} />)}
        </div>
      )}
      {tab === "relecture" && (
        <div className="m-replay">
          <TrackMap cmp={c} store={store} selected={null} onSelectCorner={() => undefined} height={Math.round(Math.min(window.innerHeight * 0.45, 420))} />
          <ReplayPanel cmp={c} store={store} />
        </div>
      )}
      {tab === "courbes" && (
        <>
          <ReplayPanel cmp={c} store={store} compact />
          <TelemetryCharts cmp={c} simple store={store} onCursor={(i) => { if (i != null && !store.get().playing) store.seek(c.cmp_trace.time[i] ?? 0, true); }} />
        </>
      )}
      {tab === "virages" && (
        <div className="m-corners">
          {c.corners.map((k) => (
            <div key={k.corner.id} className="card m-corner">
              <div className="row" style={{ justifyContent: "space-between" }}>
                <b>{k.corner.name}</b>
                <span className={`mono ${k.time_delta_s > 0.02 ? "pos" : k.time_delta_s < -0.02 ? "neg" : ""}`}>{delta(k.time_delta_s, 2)} s</span>
              </div>
              <table>
                <thead><tr><th /><th className="num">Réf.</th><th className="num">Tour</th></tr></thead>
                <tbody>
                  <tr><td>Freinage</td><td className="num">{k.reference.brake_on_m?.toFixed(0) ?? "—"} m</td><td className="num">{k.compared.brake_on_m?.toFixed(0) ?? "—"} m</td></tr>
                  <tr><td>Vitesse mini</td><td className="num">{k.reference.min_speed_kmh.toFixed(0)}</td><td className="num">{k.compared.min_speed_kmh.toFixed(0)} km/h</td></tr>
                  <tr><td>Vitesse sortie</td><td className="num">{k.reference.exit_speed_kmh.toFixed(0)}</td><td className="num">{k.compared.exit_speed_kmh.toFixed(0)} km/h</td></tr>
                  <tr><td>Remise des gaz</td><td className="num">{k.reference.throttle_on_m?.toFixed(0) ?? "—"} m</td><td className="num">{k.compared.throttle_on_m?.toFixed(0) ?? "—"} m</td></tr>
                  <tr><td>Trajectoire apex</td><td className="num" colSpan={2}>{k.apex_offset_m == null ? "—" : Math.abs(k.apex_offset_m) < 0.3 ? "identique" : `${Math.abs(k.apex_offset_m).toFixed(1)} m plus à l'${k.apex_offset_m > 0 ? "intérieur" : "extérieur"}`}</td></tr>
                </tbody>
              </table>
            </div>
          ))}
        </div>
      )}
    </>
  );
}
