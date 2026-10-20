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
const OFF_COLOR = "#ff8a00";

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
    // Portions hors piste (m) → tracés sur la trajectoire concernée.
    const spanPath = (xs: number[], ys: number[], spans: [number, number][]) =>
      spans
        .map(([a, b]) => {
          const i0 = Math.max(0, Math.round(a / c.step_m)), i1 = Math.min(xs.length - 1, Math.max(i0 + 3, Math.round(b / c.step_m)));
          return linePath(xs.slice(i0, i1 + 1), ys.slice(i0, i1 + 1), 1);
        })
        .join("");
    const cmpOff = CX && CY && c.cmp_off_track_m?.length ? spanPath(CX, CY, c.cmp_off_track_m) : "";
    const refOff = c.ref_off_track_m?.length ? spanPath(X, Y, c.ref_off_track_m) : "";
    return { band, refD, cmpD, cmpColored, cmpOff, refOff };
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
      {layers.refOff && <path d={layers.refOff} stroke={OFF_COLOR} strokeWidth={5} strokeDasharray="4 4" strokeLinecap="round" fill="none" opacity={0.8} vectorEffect="non-scaling-stroke" />}
      {layers.cmpOff && <path d={layers.cmpOff} stroke={OFF_COLOR} strokeWidth={6} strokeLinecap="round" fill="none" vectorEffect="non-scaling-stroke" />}
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
    // Numéro officiel quand le nom en contient un (« Virage 12 », « V3 »), sinon numéro d'ordre.
    const official = /(\d+(?:-\d+)?)\s*$/.exec(k.corner.name)?.[1];
    return { id: k.corner.id, num: official ?? String(num + 1), name: k.corner.name, x: X[i]!, y: Y[i]!, nx, ny, loss: k.time_delta_s };
  });
}

interface View { cx: number; cy: number; w: number }

/** Cap (degrés, repère écran) de la trajectoire autour de l'index `fi`, sur ±15 m. */
function headingAt(X: number[], Y: number[], fi: number, span: number): number {
  const n = X.length;
  const a = Math.max(0, Math.floor(fi - span)), b = Math.min(n - 1, Math.ceil(fi + span));
  return (Math.atan2(Y[b]! - Y[a]!, X[b]! - X[a]!) * 180) / Math.PI;
}

/** Petite vue d'ensemble affichée quand la carte est zoomée. */
const Overview = memo(function Overview({ geo, x, y }: { geo: Geo; x: number; y: number }) {
  const b = geo.bounds;
  const w = (b.x1 - b.x0) * 1.15, h = (b.y1 - b.y0) * 1.15;
  const s = Math.max(w, h);
  const d = useMemo(() => linePath(geo.X, geo.Y, 6), [geo]);
  return (
    <svg className="trackmap-overview" viewBox={`${(b.x0 + b.x1) / 2 - s / 2} ${(b.y0 + b.y1) / 2 - s / 2} ${s} ${s}`}>
      <path d={d} stroke="#6b6b80" strokeWidth={2} fill="none" vectorEffect="non-scaling-stroke" />
      <circle cx={x} cy={y} r={s * 0.035} fill={CMP_LINE} stroke="#fff" strokeWidth={1.5} vectorEffect="non-scaling-stroke" />
    </svg>
  );
});

export function TrackMap({ cmp, store, selected, onSelectCorner, height = 420, big, onToggleBig }: {
  cmp: Comparison;
  store: ReplayStore;
  selected: number | null;
  onSelectCorner: (id: number) => void;
  height?: number;
  big?: boolean;
  onToggleBig?: () => void;
}) {
  const geo = useMemo(() => buildGeo(cmp), [cmp]);
  const labels = useMemo(() => (geo ? cornerLabels(cmp, geo) : []), [cmp, geo]);
  const host = useRef<HTMLDivElement>(null);
  const [px, setPx] = useState(400);
  const [mode, setMode] = useState<ColorMode>("delta");
  const [follow, setFollow] = useState(false);
  /** Orientation en mode suivi : la voiture pointe toujours vers le haut. */
  const [carUp, setCarUp] = useState(true);
  /** Échelle du suivi rapproché (mètres par pixel), indépendante de la taille de la carte. */
  const [zoomMpp, setZoomMpp] = useState(0.55);
  const zoomW = zoomMpp * px;
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

  // Lecture lancée : la caméra passe automatiquement en suivi rapproché.
  const wasPlaying = useRef(false);
  useEffect(() => {
    if (rs.playing && !wasPlaying.current) setFollow(true);
    wasPlaying.current = rs.playing;
  }, [rs.playing]);

  const CX = geo?.CX && geo.CY ? geo.CX : geo?.X;
  const CY = geo?.CX && geo.CY ? geo.CY : geo?.Y;
  const car = geo && CX && CY ? { x: at(CX, ci)!, y: at(CY, ci)! } : null;
  const ghost = geo ? { x: at(geo.X, ri)!, y: at(geo.Y, ri)! } : null;

  // Cap lissé (évite les à-coups de rotation).
  const smooth = useRef<number | null>(null);
  let rot = 0;
  if (follow && carUp && geo && CX && CY) {
    const hd = headingAt(CX, CY, ci, 15 / cmp.step_m);
    const prev = smooth.current ?? hd;
    let diff = ((hd - prev + 540) % 360) - 180;
    const next = prev + diff * (rs.playing && Math.abs(diff) < 90 ? 0.18 : 1);
    smooth.current = next;
    rot = -90 - next;
  } else {
    smooth.current = null;
  }

  let v: View | null = follow && car ? { cx: car.x, cy: car.y, w: zoomW } : view ?? full;
  if (v && follow && car && carUp && smooth.current != null) {
    // Vue pilote : la voiture est placée dans le tiers bas pour voir ce qui arrive.
    const a = (smooth.current * Math.PI) / 180;
    const ahead = 0.2 * zoomW * (height / px);
    v = { ...v, cx: car.x + Math.cos(a) * ahead, cy: car.y + Math.sin(a) * ahead };
  }
  const mPerPx = v ? v.w / px : 1;
  const zoomed = !!(v && full && v.w < full.w * 0.6);

  // Déplacement à la souris / au doigt.
  const drag = useRef<{ x: number; y: number; v: View } | null>(null);
  // Pincement à deux doigts : zoom.
  const pts = useRef(new Map<number, { x: number; y: number }>());
  const pinch = useRef<number | null>(null);
  const onDown = (e: React.PointerEvent) => {
    if (!v) return;
    pts.current.set(e.pointerId, { x: e.clientX, y: e.clientY });
    drag.current = pts.current.size === 1 ? { x: e.clientX, y: e.clientY, v } : null;
    (e.currentTarget as Element).setPointerCapture?.(e.pointerId);
  };
  const onMove = (e: React.PointerEvent) => {
    if (pts.current.has(e.pointerId)) pts.current.set(e.pointerId, { x: e.clientX, y: e.clientY });
    if (pts.current.size === 2) {
      const [a, b] = [...pts.current.values()];
      const dist = Math.hypot(a!.x - b!.x, a!.y - b!.y);
      if (pinch.current) zoomBy(pinch.current / dist, (a!.x + b!.x) / 2, (a!.y + b!.y) / 2);
      pinch.current = dist;
      return;
    }
    const d = drag.current;
    if (!d) return;
    if (Math.abs(e.clientX - d.x) + Math.abs(e.clientY - d.y) > 4) {
      const k = d.v.w / px;
      setFollow(false);
      setView({ ...d.v, cx: d.v.cx - (e.clientX - d.x) * k, cy: d.v.cy - (e.clientY - d.y) * k });
    }
  };
  const onUp = (e: React.PointerEvent) => {
    pts.current.delete(e.pointerId);
    if (pts.current.size < 2) pinch.current = null;
    drag.current = null;
  };

  const zoomBy = useCallback(
    (f: number, clientX?: number, clientY?: number) => {
      if (!v || !host.current) return;
      if (follow) {
        setZoomMpp((m) => Math.max(0.08, Math.min(8, m * f)));
        return;
      }
      const r = host.current.getBoundingClientRect();
      const mx = clientX == null ? v.cx : v.cx + (clientX - r.left - r.width / 2) * (v.w / px);
      const my = clientY == null ? v.cy : v.cy + (clientY - r.top - r.height / 2) * (v.w / px);
      const w = Math.max(40, Math.min((full?.w ?? 5000) * 1.5, v.w * f));
      setView({ w, cx: mx + (v.cx - mx) * (w / v.w), cy: my + (v.cy - my) * (w / v.w) });
    },
    [v, follow, px, full],
  );
  const onWheel = useCallback(
    (e: WheelEvent) => {
      e.preventDefault();
      zoomBy(Math.exp(e.deltaY * 0.0015), e.clientX, e.clientY);
    },
    [zoomBy],
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
  const showNames = v.w < 1200;
  const zoomTo = (x: number, y: number, w: number) => { setFollow(false); setView({ cx: x, cy: y, w }); };
  const g = `rotate(${rot.toFixed(2)} ${v.cx} ${v.cy})`;
  /** Garde les textes lisibles (droits) quand la carte tourne. */
  const upright = (x: number, y: number) => `rotate(${(-rot).toFixed(2)} ${x} ${y})`;

  return (
    <div className="trackmap">
      <div className="trackmap-tools">
        <div className="seg">
          {([["delta", "Gain / perte"], ["inputs", "Frein / gaz"], ["speed", "Vitesse"]] as const).map(([k, l]) => (
            <button key={k} className={mode === k ? "on" : ""} onClick={() => setMode(k)}>{l}</button>
          ))}
        </div>
        <div className="seg">
          <button className={follow ? "on" : ""} onClick={() => setFollow((f) => !f)} title="La caméra suit la voiture (molette : zoom). Activé automatiquement en lecture.">Suivre</button>
          {follow && <button className={carUp ? "on" : ""} onClick={() => setCarUp((x) => !x)} title="La carte tourne pour que la voiture pointe vers le haut">Vue pilote</button>}
          <button onClick={() => { setFollow(false); setView(null); }} title="Voir tout le circuit">Circuit</button>
          {onToggleBig && <button className={big ? "on" : ""} onClick={onToggleBig} title="Agrandir la carte et la relecture">{big ? "Réduire" : "Agrandir"}</button>}
        </div>
      </div>
      <div
        ref={host}
        className="trackmap-canvas"
        style={{ height }}
        onPointerDown={onDown}
        onPointerMove={onMove}
        onPointerUp={onUp}
        onPointerCancel={onUp}
      >
        <svg viewBox={vb} width="100%" height={height} preserveAspectRatio="xMidYMid meet">
          <g transform={g}>
            <StaticLayers c={cmp} geo={geo} mode={mode} />
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
                  <g transform={upright(x, y)}>
                    <text x={x} y={y + 4 * mPerPx} textAnchor="middle" fontSize={(k.num.length > 2 ? 7.5 : 11) * mPerPx} fontWeight={800} fill={sel ? "#15151e" : "#ffffff"} fontFamily="Archivo Variable, sans-serif">{k.num}</text>
                    {showNames && (
                      <text x={x + 15 * mPerPx} y={y + 4 * mPerPx} fontSize={12 * mPerPx} fontWeight={700} fontStyle="italic" fill="#ffffff" stroke="#101017" strokeWidth={3 * mPerPx} paintOrder="stroke" fontFamily="Archivo Variable, sans-serif">{k.name}</text>
                    )}
                  </g>
                </g>
              );
            })}
            {ghost && <circle cx={ghost.x} cy={ghost.y} r={6 * mPerPx} fill="rgba(21,21,30,0.6)" stroke={REF_LINE} strokeWidth={2} vectorEffect="non-scaling-stroke" />}
            {car && <circle cx={car.x} cy={car.y} r={7 * mPerPx} fill={CMP_LINE} stroke="#ffffff" strokeWidth={2} vectorEffect="non-scaling-stroke" />}
          </g>
        </svg>
        {zoomed && car && <Overview geo={geo} x={car.x} y={car.y} />}
        <div className="trackmap-zoom">
          <button onPointerDown={(e) => e.stopPropagation()} onClick={() => zoomBy(0.7)} aria-label="Zoomer">+</button>
          <button onPointerDown={(e) => e.stopPropagation()} onClick={() => zoomBy(1.4)} aria-label="Dézoomer">−</button>
        </div>
      </div>
      <div className="trackmap-legend">
        <span><i style={{ background: REF_LINE }} />Référence (tour {cmp.reference.number}) · cercle vide = fantôme</span>
        <span><i style={{ background: CMP_LINE }} />Tour {cmp.compared.number}</span>
        {(cmp.cmp_off_track_m?.length > 0 || cmp.ref_off_track_m?.length > 0) && <span><i style={{ background: OFF_COLOR }} />hors piste</span>}
        {mode === "delta" && <span><i style={{ background: "#2bd576" }} />gain <i style={{ background: "#ffd400", marginLeft: 6 }} />perte</span>}
        {mode === "inputs" && <span><i style={{ background: "#ff2a1f" }} />frein <i style={{ background: "#ffd400", marginLeft: 6 }} />gaz partiel <i style={{ background: "#2bd576", marginLeft: 6 }} />plein gaz <i style={{ background: "#fff", marginLeft: 6 }} />roue libre</span>}
        {mode === "speed" && <span>{SPEED_COLORS.map((col, i) => <i key={col} style={{ background: col, marginLeft: i ? 2 : 0 }} />)} lent → rapide</span>}
      </div>
    </div>
  );
}
