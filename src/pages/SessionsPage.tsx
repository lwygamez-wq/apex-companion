import { save } from "@tauri-apps/plugin-dialog";
import { useEffect, useMemo, useRef, useState } from "react";
import type { Route } from "../App";
import { ImportActions } from "../components/ImportActions";
import { SetupChip, SetupSelect } from "../components/SetupChip";
import { api } from "../lib/api";
import { lapTime } from "../lib/format";
import { useAsync } from "../lib/hooks";
import type { SessionKind, SessionSummary } from "../lib/types";

const KIND_SHORT: Record<SessionKind, string> = {
  practice: "Essais",
  qualifying: "Qualif",
  race: "Course",
  hotlap: "Hotlap",
  private_testing: "Test",
  unknown: "—",
};

/** Session sans tour chronométré exploitable (sortie des stands, abandon) : masquée par défaut. */
const isEmpty = (s: SessionSummary) => s.best_lap_s == null && s.mean_clean_s == null && s.lap_count <= 3;

const dayLabel = (unix: number) => new Date(unix * 1000).toLocaleDateString("fr-FR", { weekday: "long", day: "numeric", month: "long" });
const hour = (unix: number) => new Date(unix * 1000).toLocaleTimeString("fr-FR", { hour: "2-digit", minute: "2-digit" });

/** Menu « ⋯ » d'une session : actions secondaires, hors de la vue. */
function RowMenu({ s, onExport, onMobile, onDelete }: { s: SessionSummary; onExport: () => void; onMobile: () => void; onDelete: () => void }) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const close = (e: MouseEvent) => !ref.current?.contains(e.target as Node) && setOpen(false);
    window.addEventListener("mousedown", close);
    return () => window.removeEventListener("mousedown", close);
  }, [open]);
  return (
    <div className="row-menu" ref={ref} onClick={(e) => e.stopPropagation()}>
      <button className="btn ghost small" aria-label={`Actions pour la session du ${hour(s.started_at_unix)}`} onClick={() => setOpen((o) => !o)}>⋯</button>
      {open && (
        <div className="row-menu-pop">
          <button onClick={() => (setOpen(false), onExport())}>Exporter pour analyse</button>
          <button onClick={() => (setOpen(false), onMobile())}>Copier pour le mobile</button>
          <button className="danger" onClick={() => (setOpen(false), onDelete())}>Supprimer</button>
        </div>
      )}
    </div>
  );
}

export function SessionsPage({ navigate }: { navigate: (r: Route) => void }) {
  const all = useAsync(() => api.sessions(), []);
  const refs = useAsync(() => api.sessions({ external: true }), []);
  const [track, setTrack] = useState("");
  const [kind, setKind] = useState<SessionKind | "">("");
  const [showEmpty, setShowEmpty] = useState(false);
  const [msg, setMsg] = useState<string | null>(null);

  const tracks = useMemo(() => [...new Set((all.data ?? []).map((s) => s.track))].sort(), [all.data]);
  const filtered = useMemo(() => (all.data ?? []).filter((s) => (!track || s.track === track) && (!kind || s.kind === kind)), [all.data, track, kind]);
  const empties = filtered.filter(isEmpty).length;
  const visible = showEmpty ? filtered : filtered.filter((s) => !isEmpty(s));
  const days = useMemo(() => {
    const m = new Map<string, SessionSummary[]>();
    for (const s of visible) {
      const k = dayLabel(s.started_at_unix);
      m.set(k, [...(m.get(k) ?? []), s]);
    }
    return [...m.entries()];
  }, [visible]);
  const untyped = (refs.data ?? []).filter((r) => r.setup == null).length;

  const reload = () => (all.reload(), refs.reload());
  const report = (p: Promise<string>) => p.then(setMsg, (e: unknown) => setMsg(e instanceof Error ? e.message : String(e)));
  const exportOne = (s: SessionSummary) =>
    report(
      (async () => {
        const date = new Date(s.started_at_unix * 1000).toISOString().slice(0, 10);
        const path = await save({ defaultPath: `apex-${s.track}-${date}`.replace(/[^\w.-]+/g, "_") + ".apex.json.gz", filters: [{ name: "Export Apex", extensions: ["gz"] }] });
        if (!path) return "Export annulé.";
        const r = await api.exportSession(s.id, path);
        return `Export prêt (${(r.bytes / 1e6).toFixed(1)} Mo, ${r.laps} tours).`;
      })(),
    );
  const toMobile = (s: SessionSummary) =>
    report(api.exportToMobile(s.id).then((p) => (p ? `Copiée pour le mobile : ${p}` : "Choisis d'abord le dossier « Copie automatique pour le mobile » dans les Réglages.")));

  return (
    <>
      <div className="page-title">
        <h1>Sessions</h1>
        <ImportActions compact onDone={reload} />
      </div>
      {msg && <div className="warning" onClick={() => setMsg(null)}>{msg}</div>}

      <div className="toolbar">
        <select value={track} onChange={(e) => setTrack(e.target.value)} aria-label="Circuit">
          <option value="">Tous les circuits</option>
          {tracks.map((t) => <option key={t}>{t}</option>)}
        </select>
        <div className="seg">
          {([["", "Tout"], ["practice", "Essais"], ["qualifying", "Qualif"], ["race", "Course"]] as const).map(([k, l]) => (
            <button key={k} className={kind === k ? "on" : ""} onClick={() => setKind(k as SessionKind | "")}>{l}</button>
          ))}
        </div>
        {empties > 0 && (
          <label className="muted toolbar-check">
            <input type="checkbox" checked={showEmpty} onChange={(e) => setShowEmpty(e.target.checked)} /> sessions sans tour chronométré ({empties})
          </label>
        )}
      </div>

      {all.error && <div className="error">{all.error}</div>}
      {all.data?.length === 0 && <div className="card empty"><h2>Aucune session</h2><p>Importe tes fichiers iRacing, ou laisse la synchro automatique faire (iRacing et Le Mans Ultimate).</p><ImportActions onDone={reload} /></div>}

      {days.map(([day, list]) => (
        <section key={day} className="day">
          <h3 className="day-title">{day}</h3>
          <div className="session-list">
            {list.map((s) => (
              <div key={s.id} className={`session-row ${isEmpty(s) ? "dim" : ""}`} role="button" tabIndex={0} onClick={() => navigate({ page: "analysis", sessionId: s.id })} onKeyDown={(e) => e.key === "Enter" && navigate({ page: "analysis", sessionId: s.id })}>
                <span className="sr-time mono">{hour(s.started_at_unix)}</span>
                <span className="sr-where">
                  <b>{s.track}</b>
                  <span className="muted">{s.car}</span>
                </span>
                <span className="sr-kind">
                  <span className="chip">{KIND_SHORT[s.kind]}</span>
                  {s.setup ? <SetupChip setup={s.setup} /> : s.sim === "iracing" && (
                    <span onClick={(e) => e.stopPropagation()}><SetupSelect value={null} onChange={(v) => void api.setSessionSetup(s.id, v).then(reload)} /></span>
                  )}
                </span>
                <span className="sr-best mono">{lapTime(s.best_lap_s)}</span>
                <span className="sr-laps muted">{s.clean_lap_count}/{s.lap_count} propres</span>
                <RowMenu s={s} onExport={() => exportOne(s)} onMobile={() => toMobile(s)} onDelete={() => void api.deleteSession(s.id).then(reload)} />
              </div>
            ))}
          </div>
        </section>
      ))}

      {(refs.data?.length ?? 0) > 0 && (
        <details className="card refs" open={untyped > 0}>
          <summary>
            Références d'autres pilotes ({refs.data!.length})
            {untyped > 0 && <span className="chip warn" style={{ marginLeft: 8 }}>{untyped} sans série</span>}
          </summary>
          <p className="muted small-note">Une référence <b>Fixed</b> ne sert qu'à tes sessions fixed, une <b>Open</b> qu'à tes sessions open. Le fichier Garage 61 ne dit pas la série : choisis-la.</p>
          {refs.data!.map((r) => (
            <div key={r.id} className="ref-row">
              <b>{r.external_driver}</b>
              <span className="muted">{r.track} · {r.car}</span>
              <span className="mono">{lapTime(r.best_lap_s ?? null)}</span>
              <SetupSelect value={r.setup} onChange={(v) => void api.setSessionSetup(r.id, v).then(() => refs.reload())} />
              <button className="btn ghost small" onClick={() => void api.deleteSession(r.id).then(() => refs.reload())}>Retirer</button>
            </div>
          ))}
        </details>
      )}
    </>
  );
}
