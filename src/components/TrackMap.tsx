import { memo, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { at, positionsAt, useReplay, type ReplayStore } from "../lib/replay";
import type { Comparison } from "../lib/types";

/**
 * Carte du circuit interactive : trajectoires réelles (GPS) de la référence et du tour analysé,
 * relecture animée (voiture + fantôme de la référence au même instant), zoom molette,
 * déplacement à la souris, caméra qui suit la voiture.
 */

export type ColorMode = "delta" | "inputs" | "speed";

const REF_LINE = "rgba(201,201,212,0.85)";
const CMP_LINE = "#e10600";

interface Geo {
  X: number[];
  Y: number[];
  CX: number[] | null;
  CY: number[] | null;
  bounds: { x0: number; y0: number; x1: number; y1: number };
}

function buildGeo(c: Comparison): Geo | null {
  const r = c.ref_trace;
  if (!r.x || !r.y || r.x.length < 3) return null;
  const X = r.x;
  const Y = r.y.map((v) => -v);
  let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
  for (let i = 0; i < X.length; i++) {
    x0 = Math.min(x0, X[i]!); x1 = Math.max(x1, X[i]!);
    y0 = Math.min(y0, Y[i]!); y1 = Math.max(y1, Y[i]!);
  }
  const cx = c.cmp_trace.x;
  const cy = c.cmp_trace.y;
  return { X, Y, CX: cx ?? null, CY: cy ? cy.map((v) => -v) : null, bounds: { x0, y0, x1, y1 } };
}

/** Regroupe les segments par couleur : quelques <path> au lieu de milliers. */
function bucketPaths(X: number[], Y: number[], step: number, colorOf: (i: number) => string): [string, string][] {
  const m = new Map<string, string[]>();
  for (let i = 0; i + step < X.length; i += step) {
    const col = colorOf(i);
    const p = `M${X[i]!.toFixed(1)} ${Y[i]!.toFixed(1)}L${X[i + step]!.toFixed(1)} ${Y[i + step]!.toFixed(1)}`;
    const a = m.get(col);
    if (a) a.push(p);
    else m.set(col, [p]);
  }
  return [...m.entries()].map(([k, v]) => [k, v.join("")]);
}

function linePath(X: number[], Y: number[], step = 2): string {
  let d = `M${X[0]!.toFixed(1)} ${Y[0]!.toFixed(1)}`;
  for (let i = step; i < X.length; i += step) d += `L${X[i]!.toFixed(1)} ${Y[i]!.toFixed(1)}`;
  return d;
}

const SPEED_COLORS = ["#2f6bff", "#3fa7ff", "#2bd576", "#ffd400", "#ff8a00", "#e10600"];

/** Couches fixes (ne dépendent pas du temps) : mémorisées. */
const StaticLayers = memo(function StaticLayers({ c, geo, mode }: { c: Comparison; geo: Geo; mode: ColorMode }) {
  const layers = useMemo(() => {
    const { X, Y, CX, CY } = geo;
    const step = Math.max(1, Math.round(4 / c.step_m));
    let band: [string, string][] = [];
    let cmpColored: [string, string][] | null = null;
    if (mode === "delta") {
      const s20 = Math.max(1, Math.round(20 / c.step_m));
      band = bucketPaths(X, Y, s20, (i) => {
        const j = Math.min(i + s20, c.delta_s.length - 1);
        const rate = ((c.delta_s[j] ?? 0) - (c.delta_s[i] ?? 0)) / ((j - i) * c.step_m || 1);
        const k = Math.max(-1, Math.min(1, rate * 400));
        const q = Math.round(Math.abs(k) * 4) / 4;
        if (k > 0.05) return `rgba(255,212,0,${(0.3 + 0.7 * q).toFixed(2)})`;
        if (k < -0.05) return `rgba(43,213,118,${(0.3 + 0.7 * q).toFixed(2)})`;
        return "#34344a";
      });
    } else {
      band = [["#2a2a3a", linePath(X, Y)]];
      if (CX && CY) {
        const t = c.cmp_trace;
        cmpColored = bucketPaths(CX, CY, step, (i) => {
          if (mode === "inputs") {
            const b = t.brake[i] ?? 0;
            const th = t.throttle[i] ?? 0;
            if (b > 0.05) return b > 0.5 ? "#ff2a1f" : "#ff7a6e";
            if (th > 0.95) return "#2bd576";
            if (th > 0.05) return "#ffd400";
            return "#ffffff";
          }
          const v = t.speed_kmh[i] ?? 0;
          return SPEED_COLORS[Math.max(0, Math.min(SPEED_COLORS.length - 1, Math.floor((v - 60) / 45)))]!;
        });
      }
    }
    const refD = linePath(X, Y, 1);
    const cmpD = CX && CY ? linePath(CX, CY, 1) : null;
    return { band, refD, cmpD, cmpColored };
  }, [c, geo, mode]);

  return (
    <g>
      {layers.band.map(([col, d]) => (
        <path key={col} d={d} stroke={col} strokeWidth={mode === "delta" ? 12 : 9} strokeLinecap="round" fill="none" vectorEffect="non-scaling-stroke" />
      ))}
      <path d={layers.refD} stroke={REF_LINE} strokeWidth={1.4} fill="none" vectorEffect="non-scaling-stroke" />
      {layers.cmpColored
        ? layers.cmpColored.map(([col, d]) => <path key={col} d={d} stroke={col} strokeWidth={3} strokeLinecap="round" fill="none" vectorEffect="non-scaling-stroke" />)
        : layers.cmpD && <path d={layers.cmpD} stroke={CMP_LINE} strokeWidth={1.6} fill="none" vectorEffect="non-scaling-stroke" />}
    </g>
  );
});

/** Repères des virages, placés à l'extérieur de la courbe. */
function cornerLabels(c: Comparison, geo: Geo) {
  const { X, Y } = geo;
  const n = X.length;
  const idx = (m: number) => Math.max(0, Math.min(n - 1, Math.round(m / c.step_m)));
  const tan = (i: number) => {
    const a = Math.max(0, i - 6), b = Math.min(n - 1, i + 6);
    const dx = X[b]! - X[a]!, dy = Y[b]! - Y[a]!;
    const l = Math.hypot(dx, dy) || 1;
    return [dx / l, dy / l] as const;
  };
  return c.corners.map((k, num) => {
    const i = idx(k.corner.apex_m);
    const [ax, ay] = tan(idx(k.corner.apex_m - 40));
    const [bx, by] = tan(idx(k.corner.apex_m + 40));
    const turn = Math.sign(ax * by - ay * bx) || 1; // repère écran (y vers le bas)
    const [tx, ty] = tan(i);
    // Normale vers l'extérieur du virage.
    const nx = -ty * -turn, ny = tx * -turn;
    return { id: k.corner.id, num: num + 1, name: k.corner.name, x: X[i]!, y: Y[i]!, nx, ny, loss: k.time_delta_s };
  });
}

interface View { cx: number; cy: number; w: number }

export function TrackMap({ cmp, store, selected, onSelectCorner, height = 420 }: {
  cmp: Comparison;
  store: ReplayStore;
  selected: number | null;
  onSelectCorner: (id: number) => void;
  height?: number;
}) {
  const geo = useMemo(() => buildGeo(cmp), [cmp]);
  const labels = useMemo(() => (geo ? cornerLabels(cmp, geo) : []), [cmp, geo]);
  const host = useRef<HTMLDivElement>(null);
  const [px, setPx] = useState(400);
  const [mode, setMode] = useState<ColorMode>("delta");
  const [follow, setFollow] = useState(false);
  const [zoomW, setZoomW] = useState(260);
  const full = useMemo<View | null>(() => {
    if (!geo) return null;
    const b = geo.bounds;
    const w = Math.max(b.x1 - b.x0, (b.y1 - b.y0) * (px / height)) * 1.18 + 60 * ((b.x1 - b.x0) / px);
    return { cx: (b.x0 + b.x1) / 2, cy: (b.y0 + b.y1) / 2, w };
  }, [geo, px, height]);
  const [view, setView] = useState<View | null>(null);
  useEffect(() => setView(null), [cmp]);

  useEffect(() => {
    const el = host.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setPx(el.clientWidth || 400));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const rs = useReplay(store);
  const { ci, ri } = positionsAt(cmp, rs.t);

  const car = geo?.CX && geo.CY ? { x: at(geo.CX, ci)!, y: at(geo.CY, ci)! } : geo ? { x: at(geo.X, ci)!, y: at(geo.Y, ci)! } : null;
  const ghost = geo ? { x: at(geo.X, ri)!, y: at(geo.Y, ri)! } : null;

  const v: View | null = follow && car ? { cx: car.x, cy: car.y, w: zoomW } : view ?? full;
  const mPerPx = v ? v.w / px : 1;

  // Déplacement à la souris.
  const drag = useRef<{ x: number; y: number; v: View } | null>(null);
  const onDown = (e: React.PointerEvent) => {
    if (!v) return;
    drag.current = { x: e.clientX, y: e.clientY, v };
    (e.target as Element).setPointerCapture?.(e.pointerId);
  };
  const onMove = (e: React.PointerEvent) => {
    const d = drag.current;
    if (!d) return;
    const dx = (e.clientX - d.x) * (d.v.w / px), dy = (e.clientY - d.y) * (d.v.w / px);
    if (Math.abs(e.clientX - d.x) + Math.abs(e.clientY - d.y) > 3) {
      setFollow(false);
      setView({ ...d.v, cx: d.v.cx - dx, cy: d.v.cy - dy });
    }
  };
  const onUp = () => { drag.current = null; };

  const onWheel = useCallback(
    (e: WheelEvent) => {
      if (!v || !host.current) return;
      e.preventDefault();
      const f = Math.exp(e.deltaY * 0.0015);
      if (follow) {
        setZoomW((w) => Math.max(40, Math.min(3000, w * f)));
        return;
      }
      const r = host.current.getBoundingClientRect();
      const mx = v.cx + (e.clientX - r.left - r.width / 2) * (v.w / px);
      const my = v.cy + (e.clientY - r.top - r.height / 2) * (v.w / px);
      const w = Math.max(40, Math.min((full?.w ?? 5000) * 1.5, v.w * f));
      setView({ w, cx: mx + (v.cx - mx) * (w / v.w), cy: my + (v.cy - my) * (w / v.w) });
    },
    [v, follow, px, full],
  );
  useEffect(() => {
    const el = host.current;
    if (!el) return;
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, [onWheel]);

  if (!geo || !v) {
    return <div className="card empty" style={{ padding: 30 }}>Carte non disponible : la source ne fournit pas la position du véhicule.</div>;
  }

  const hView = v.w * (height / px);
  const vb = `${v.cx - v.w / 2} ${v.cy - hView / 2} ${v.w} ${hView}`;
  const showNames = v.w < 900;
  const zoomTo = (x: number, y: number, w: number) => { setFollow(false); setView({ cx: x, cy: y, w }); };

  return (
    <div className="trackmap">
      <div className="trackmap-tools">
        <div className="seg">
          {([["delta", "Gain / perte"], ["inputs", "Frein / gaz"], ["speed", "Vitesse"]] as const).map(([k, l]) => (
            <button key={k} className={mode === k ? "on" : ""} onClick={() => setMode(k)}>{l}</button>
          ))}
        </div>
        <div className="seg">
          <button className={follow ? "on" : ""} onClick={() => setFollow((f) => !f)} title="La caméra suit la voiture (molette : zoom)">Suivre</button>
          <button onClick={() => { setFollow(false); setView(null); }} title="Voir tout le circuit">Circuit</button>
        </div>
      </div>
      <div
        ref={host}
        className="trackmap-canvas"
        style={{ height }}
        onPointerDown={onDown}
        onPointerMove={onMove}
        onPointerUp={onUp}
        onPointerLeave={onUp}
      >
        <svg viewBox={vb} width="100%" height={height} preserveAspectRatio="xMidYMid meet">
          <StaticLayers c={cmp} geo={geo} mode={mode} />
          {/* Ligne de départ */}
          {(() => {
            const dx = geo.X[3]! - geo.X[0]!, dy = geo.Y[3]! - geo.Y[0]!;
            const l = Math.hypot(dx, dy) || 1;
            const s = 9 * mPerPx;
            return <line x1={geo.X[0]! - (dy / l) * s} y1={geo.Y[0]! + (dx / l) * s} x2={geo.X[0]! + (dy / l) * s} y2={geo.Y[0]! - (dx / l) * s} stroke="#fff" strokeWidth={3} vectorEffect="non-scaling-stroke" />;
          })()}
          {labels.map((k) => {
            const off = 22 * mPerPx;
            const x = k.x + k.nx * off, y = k.y + k.ny * off;
            const sel = selected === k.id;
            const r = (sel ? 12 : 10) * mPerPx;
            return (
              <g key={k.id} style={{ cursor: "pointer" }} onPointerDown={(e) => e.stopPropagation()} onClick={() => { onSelectCorner(k.id); zoomTo(k.x, k.y, 380); }}>
                <title>{k.name}</title>
                <circle cx={x} cy={y} r={r} fill={sel ? "#ffffff" : "#15151e"} stroke="#ffffff" strokeWidth={1.5} vectorEffect="non-scaling-stroke" />
                <text x={x} y={y + 4 * mPerPx} textAnchor="middle" fontSize={11 * mPerPx} fontWeight={800} fill={sel ? "#15151e" : "#ffffff"} fontFamily="Archivo Variable, sans-serif">{k.num}</text>
                {showNames && (
                  <text x={x + (k.nx >= 0 ? 1 : -1) * 15 * mPerPx} y={y + 4 * mPerPx} textAnchor={k.nx >= 0 ? "start" : "end"} fontSize={12 * mPerPx} fontWeight={700} fontStyle="italic" fill="#ffffff" stroke="#15151e" strokeWidth={3 * mPerPx} paintOrder="stroke" fontFamily="Archivo Variable, sans-serif">{k.name}</text>
                )}
              </g>
            );
          })}
          {ghost && <circle cx={ghost.x} cy={ghost.y} r={6 * mPerPx} fill="rgba(21,21,30,0.6)" stroke={REF_LINE} strokeWidth={2} vectorEffect="non-scaling-stroke" />}
          {car && <circle cx={car.x} cy={car.y} r={7 * mPerPx} fill={CMP_LINE} stroke="#ffffff" strokeWidth={2} vectorEffect="non-scaling-stroke" />}
        </svg>
      </div>
      <div className="trackmap-legend">
        <span><i style={{ background: REF_LINE }} />Référence (tour {cmp.reference.number}) · cercle vide = fantôme</span>
        <span><i style={{ background: CMP_LINE }} />Tour {cmp.compared.number}</span>
        {mode === "delta" && <span><i style={{ background: "#2bd576" }} />gain <i style={{ background: "#ffd400", marginLeft: 6 }} />perte</span>}
        {mode === "inputs" && <span><i style={{ background: "#ff2a1f" }} />frein <i style={{ background: "#ffd400", marginLeft: 6 }} />gaz partiel <i style={{ background: "#2bd576", marginLeft: 6 }} />plein gaz <i style={{ background: "#fff", marginLeft: 6 }} />roue libre</span>}
        {mode === "speed" && <span>{SPEED_COLORS.map((col, i) => <i key={col} style={{ background: col, marginLeft: i ? 2 : 0 }} />)} lent → rapide</span>}
      </div>
    </div>
  );
}
