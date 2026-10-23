import { useEffect, useMemo, useRef, useState } from "react";
import { gapMeters, interp, type Axis, type GapUnit } from "../lib/compareView";
import { lapTime } from "../lib/format";
import { at, positionsAt, useReplay, type ReplayStore } from "../lib/replay";
import type { Comparison, Resampled, TrackOverlay } from "../lib/types";
import { TelemetryCharts } from "./TelemetryCharts";
import { TrackMap } from "./TrackMap";

const SPEEDS = [0.25, 0.5, 1, 2];

/** Carte pilote : temps du tour et valeurs instantanées (gaz, frein, vitesse, rapport). */
function DriverCard({ name, lap, tr, i, color, dashed }: { name: string; lap: number | null; tr: Resampled; i: number; color: string; dashed?: boolean }) {
  const th = Math.max(0, Math.min(1, at(tr.throttle, i) ?? 0));
  const br = Math.max(0, Math.min(1, at(tr.brake, i) ?? 0));
  const g = at(tr.gear, i);
  return (
    <div className="studio-driver">
      <div className="sd-head">
        <i className={dashed ? "swatch ref" : "swatch cmp"} style={dashed ? undefined : { borderColor: color }} />
        <b>{name}</b>
        <span className="mono sd-lap">{lapTime(lap)}</span>
      </div>
      <div className="sd-vals">
        <div className="sd-bar" title="Gaz"><i style={{ width: `${th * 100}%`, background: "#2bd576" }} /><span>{Math.round(th * 100)} %</span></div>
        <div className="sd-bar" title="Frein"><i style={{ width: `${br * 100}%`, background: "#ff3b30" }} /><span>{Math.round(br * 100)} %</span></div>
        <span className="mono sd-speed">{Math.round(at(tr.speed_kmh, i) ?? 0)}<small> km/h</small></span>
        {g != null && <span className="mono sd-gear">{Math.round(g) || "N"}</span>}
      </div>
    </div>
  );
}

/**
 * Relecture plein écran : la carte (caméra qui suit les deux voitures, lignes et pédales) à gauche,
 * toutes les courbes à droite avec le curseur qui suit la lecture, commandes en bas.
 */
export function ReplayStudio({ cmp, store, title, onClose, overlay = null }: { cmp: Comparison; store: ReplayStore; title: string; onClose: () => void; overlay?: TrackOverlay | null }) {
  const s = useReplay(store);
  const { ci, ri } = positionsAt(cmp, s.t, s.ghost);
  const [axis, setAxis] = useState<Axis>("distance");
  const [gapUnit, setGapUnit] = useState<GapUnit>("s");
  const [seg, setSeg] = useState<number>(-1);
  const gm = useMemo(() => gapMeters(cmp), [cmp]);
  const main = useRef<HTMLDivElement>(null);
  const [h, setH] = useState(600);

  // Hauteur disponible pour la carte et les courbes.
  useEffect(() => {
    const el = main.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setH(el.clientHeight));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  // Courbes : tour entier avec curseur (comme une vidéo) ; on rétablit le réglage en sortant.
  useEffect(() => {
    const prev = store.get().chartFollow;
    store.setChartFollow(false);
    return () => store.setChartFollow(prev);
  }, [store]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement | null)?.tagName;
      if (tag === "INPUT" || tag === "SELECT") return;
      if (e.key === "Escape") onClose();
      else if (e.code === "Space") {
        e.preventDefault();
        store.toggle();
      } else if (e.code === "ArrowRight") store.seek(store.get().t + (e.shiftKey ? 5 : 1));
      else if (e.code === "ArrowLeft") store.seek(store.get().t - (e.shiftKey ? 5 : 1));
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [store, onClose]);

  // Segments : tour complet ou un virage (on se place 150 m avant son entrée).
  const corners = cmp.corners;
  const goSeg = (k: number) => {
    const n = ((k + 1 + corners.length + 1) % (corners.length + 1)) - 1;
    setSeg(n);
    if (n < 0) {
      store.seek(store.start);
      return;
    }
    const c = corners[n]!;
    const t = interp(cmp.cmp_trace.dist, cmp.cmp_trace.time, Math.max(0, c.corner.entry_m - 150));
    store.pause();
    if (t != null) store.seek(t);
  };
  const segDelta = seg < 0 ? (cmp.delta_s[cmp.delta_s.length - 1] ?? 0) : corners[seg]!.time_delta_s;

  const gapS = at(cmp.delta_s, ci) ?? 0;
  const gapM = at(gm.map((v) => v ?? 0), ci) ?? 0;
  const where = ci * cmp.step_m;
  const inCorner = corners.find((k) => where >= k.corner.entry_m && where <= k.corner.exit_m);
  const refName = cmp.reference.driver ?? `Réf. tour ${cmp.reference.number}`;

  return (
    <div className="studio">
      <div className="studio-top">
        <div className="studio-title">
          <b>{title}</b>
          <span className="muted">{inCorner ? inCorner.corner.name : `${(where / 1000).toFixed(2)} km`}</span>
        </div>
        <DriverCard name={refName} lap={cmp.reference.lap_time_s} tr={cmp.ref_trace} i={ri} color="#8fb4ff" dashed />
        <DriverCard name={`Toi, tour ${cmp.compared.number}`} lap={cmp.compared.lap_time_s} tr={cmp.cmp_trace} i={ci} color="#ff3b30" />
        <button className="btn ghost" onClick={onClose} title="Échap">✕ Fermer</button>
      </div>
      <div className="studio-main" ref={main}>
        <div className="studio-map">
          <TrackMap cmp={cmp} store={store} selected={seg >= 0 ? corners[seg]!.corner.id : null} onSelectCorner={(id) => goSeg(corners.findIndex((k) => k.corner.id === id))} height={Math.max(300, h - 4)} studio overlay={overlay} />
          <div className="studio-seg" onPointerDown={(e) => e.stopPropagation()}>
            <button onClick={() => goSeg(seg - 1)} aria-label="Segment précédent">‹</button>
            <div>
              <b>{seg < 0 ? "Tour complet" : corners[seg]!.corner.name}</b>
              <span className={`mono ${segDelta > 0.005 ? "pos" : segDelta < -0.005 ? "neg" : ""}`}>{segDelta > 0 ? "+" : segDelta < 0 ? "−" : "±"}{Math.abs(segDelta).toFixed(3)} s</span>
            </div>
            <button onClick={() => goSeg(seg + 1)} aria-label="Segment suivant">›</button>
          </div>
        </div>
        <div className="studio-charts">
          <TelemetryCharts cmp={cmp} store={store} variant="studio" fitHeight={h - 8} axis={axis} gapUnit={gapUnit} selected={seg >= 0 ? corners[seg]!.corner.id : null} onCursorTime={(t) => { if (t != null && !store.get().playing) store.seek(t, true); }} />
        </div>
      </div>
      <div className="studio-bottom">
        <button className="studio-play" onClick={() => store.toggle()} title="Espace">{s.playing ? "❚❚" : "▶"}</button>
        <span className="mono studio-clock">{lapTime(s.t - store.start)}</span>
        <input
          type="range"
          className="studio-scrub"
          min={store.start}
          max={store.end}
          step={0.01}
          value={s.t}
          onChange={(e) => {
            store.pause();
            store.seek(Number(e.target.value));
          }}
        />
        <span className="studio-delta">
          Écart <b className={`mono ${gapS > 0.005 ? "pos" : gapS < -0.005 ? "neg" : ""}`}>{gapS > 0 ? "+" : gapS < 0 ? "−" : "±"}{Math.abs(gapS).toFixed(3)} s</b>
          <span className="muted"> ↔ </span>
          <b className="mono">{gapM > 0.5 ? "+" : gapM < -0.5 ? "−" : "±"}{Math.abs(gapM).toFixed(0)} m</b>
        </span>
        <div className="seg">
          {SPEEDS.map((v) => <button key={v} className={s.speed === v ? "on" : ""} onClick={() => store.setSpeed(v)}>{v}×</button>)}
        </div>
        <div className="seg" title="Voiture de la référence : au même instant, ou à ta hauteur pour comparer freinages et gaz">
          <span className="seg-label">Réf.</span>
          <button className={s.ghost === "time" ? "on" : ""} onClick={() => store.setGhost("time")}>Même temps</button>
          <button className={s.ghost === "distance" ? "on" : ""} onClick={() => store.setGhost("distance")}>Côte à côte</button>
        </div>
        <div className="seg" title="Axe des courbes">
          <button className={axis === "time" ? "on" : ""} onClick={() => setAxis("time")}>Temps</button>
          <button className={axis === "distance" ? "on" : ""} onClick={() => setAxis("distance")}>Distance</button>
        </div>
        <div className="seg" title="Unité de l'écart">
          <button className={gapUnit === "s" ? "on" : ""} onClick={() => setGapUnit("s")}>s</button>
          <button className={gapUnit === "m" ? "on" : ""} onClick={() => setGapUnit("m")}>m</button>
        </div>
      </div>
    </div>
  );
}
