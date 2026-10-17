import { useEffect } from "react";
import { delta, lapTime } from "../lib/format";
import { at, indexAtTime, positionsAt, useReplay, type ReplayStore } from "../lib/replay";
import type { Comparison } from "../lib/types";

const SPEEDS = [0.25, 0.5, 1, 2];

function Bar({ v, color }: { v: number | null; color: string }) {
  const p = Math.max(0, Math.min(1, v ?? 0)) * 100;
  return (
    <div className="rp-bar">
      <div style={{ width: `${p}%`, background: color }} />
      <span>{v == null ? "—" : `${p.toFixed(0)} %`}</span>
    </div>
  );
}

/** Lecture du tour : commandes, curseur temporel et valeurs instantanées des deux tours. */
export function ReplayPanel({ cmp, store }: { cmp: Comparison; store: ReplayStore }) {
  const s = useReplay(store);
  const { ci } = positionsAt(cmp, s.t);
  const r = cmp.ref_trace;
  const c = cmp.cmp_trace;
  // Valeurs comparées au même endroit du circuit (comme sur les courbes).
  const row = (label: string, fr: (i: number) => string, fc: (i: number) => string) => (
    <tr>
      <th>{label}</th>
      <td className="num">{fr(ci)}</td>
      <td className="num">{fc(ci)}</td>
    </tr>
  );
  const num = (a: number[] | null, digits = 0, unit = "") => (i: number) => {
    const v = at(a, i);
    return v == null ? "—" : `${v.toFixed(digits)}${unit}`;
  };
  const gap = at(cmp.delta_s, ci) ?? 0;
  // Distance d'avance du fantôme (même instant).
  const ri = indexAtTime(r.time, s.t);
  const aheadM = (ri - ci) * cmp.step_m;
  const off = at(cmp.line_offset_m, ci);
  const distM = ci * cmp.step_m;
  const corner = cmp.corners.find((k) => distM >= k.corner.entry_m && distM <= k.corner.exit_m);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement | null)?.tagName;
      if (tag === "INPUT" || tag === "SELECT" || tag === "TEXTAREA") return;
      if (e.code === "Space") {
        e.preventDefault();
        store.toggle();
      } else if (e.code === "ArrowRight") store.seek(store.get().t + (e.shiftKey ? 5 : 1));
      else if (e.code === "ArrowLeft") store.seek(store.get().t - (e.shiftKey ? 5 : 1));
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [store]);

  return (
    <div className="card replay">
      <div className="row" style={{ justifyContent: "space-between" }}>
        <div className="row">
          <button className="btn primary" onClick={() => store.toggle()} title="Espace">{s.playing ? "❚❚ Pause" : "▶ Lecture"}</button>
          <span className="mono rp-clock">{lapTime(s.t - store.start)}</span>
        </div>
        <div className="seg">
          {SPEEDS.map((v) => (
            <button key={v} className={s.speed === v ? "on" : ""} onClick={() => store.setSpeed(v)}>{v}×</button>
          ))}
        </div>
      </div>
      <input
        type="range"
        className="rp-scrub"
        min={store.start}
        max={store.end}
        step={0.01}
        value={s.t}
        onChange={(e) => {
          store.pause();
          store.seek(Number(e.target.value));
        }}
      />
      <div className="row muted" style={{ justifyContent: "space-between", fontSize: 12, marginBottom: 8 }}>
        <span>{(distM / 1000).toFixed(2)} km{corner ? ` · ${corner.corner.name}` : ""}</span>
        <span>← → : 1 s · Maj : 5 s</span>
      </div>
      <table className="rp-table">
        <thead>
          <tr><th /><th className="num">Réf. ({cmp.reference.number})</th><th className="num">Tour {cmp.compared.number}</th></tr>
        </thead>
        <tbody>
          {row("Vitesse", num(r.speed_kmh, 0, " km/h"), num(c.speed_kmh, 0, " km/h"))}
          {r.gear && c.gear && row("Rapport", num(r.gear), num(c.gear))}
          <tr><th>Gaz</th><td><Bar v={at(r.throttle, ci)} color="#2bd576" /></td><td><Bar v={at(c.throttle, ci)} color="#2bd576" /></td></tr>
          <tr><th>Frein</th><td><Bar v={at(r.brake, ci)} color="#e10600" /></td><td><Bar v={at(c.brake, ci)} color="#e10600" /></td></tr>
          {r.steer_deg && c.steer_deg && row("Volant", num(r.steer_deg, 0, "°"), num(c.steer_deg, 0, "°"))}
        </tbody>
      </table>
      <div className="rp-facts">
        <div>
          <span className="muted">Écart cumulé</span>
          <b className={`mono ${gap > 0.005 ? "pos" : gap < -0.005 ? "neg" : ""}`}>{delta(gap)} s</b>
        </div>
        <div>
          <span className="muted">Fantôme</span>
          <b className="mono">{Math.abs(aheadM) < 1 ? "à hauteur" : `${Math.abs(aheadM).toFixed(0)} m ${aheadM > 0 ? "devant" : "derrière"}`}</b>
        </div>
        <div>
          <span className="muted">Trajectoire</span>
          <b className="mono">{off == null ? "—" : Math.abs(off) < 0.3 ? "identique" : `${Math.abs(off).toFixed(1)} m à ${off > 0 ? "gauche" : "droite"}`}</b>
        </div>
      </div>
    </div>
  );
}
