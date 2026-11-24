import { memo, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { at, positionsAt, useReplay, type ReplayStore } from "../lib/replay";
import { lapTime } from "../lib/format";
import { mix } from "../lib/palette";
import { overlaps, placeLabels, type Box } from "../lib/mapLabels";
import { brakeGapText, lossText, priorityCorners } from "../lib/braking";
import type { Comparison, TrackOverlay } from "../lib/types";

/**
 * Carte du circuit interactive : trajectoires réelles (GPS) de la référence et du tour analysé,
 * relecture animée (voiture + fantôme de la référence au même instant), zoom molette,
 * déplacement à la souris, caméra qui suit la voiture.
 */

export type ColorMode = "delta" | "inputs" | "speed" | "studio";
/** Taille de la carte : dans la page, moitié de l'écran, ou plein écran. */
export type MapSize = "normal" | "half" | "full";

const REF_LINE = "var(--ref)";
const CMP_LINE = "var(--info)";
const BRAKE = "var(--pedal-brake)";
const GAS = "var(--pedal-full)";

/**
 * Voiture vue de dessus : silhouette générique de prototype fermé (≈ 4,9 × 2,0 m, avant vers +x), sans
 * marque ni modèle réel. Feux arrière allumés au freinage. Le fantôme (référence) est en contour pointillé.
 */
function Car({ x, y, heading, scale, body, ghost, braking }: { x: number; y: number; heading: number; scale: number; body: string; ghost?: boolean; braking: boolean }) {
  const line = ghost ? body : "var(--text)";
  return (
    <g transform={`translate(${x.toFixed(2)} ${y.toFixed(2)}) rotate(${heading.toFixed(1)}) scale(${scale.toFixed(4)})`} style={{ pointerEvents: "none" }}>
      {!ghost && <ellipse cx={0} cy={0} rx={3.2} ry={1.6} fill="var(--car-shadow)" />}
      {/* caisse : nez arrondi, flancs droits, poupe large */}
      <path
        d="M-2.45 -1 L1.2 -1 Q2.3 -0.95 2.55 -0.35 L2.6 0 L2.55 0.35 Q2.3 0.95 1.2 1 L-2.45 1 Q-2.6 0 -2.45 -1 Z"
        fill={ghost ? mix("ref", 0.3) : body}
        stroke={line}
        strokeWidth={ghost ? 0.14 : 0.1}
        strokeDasharray={ghost ? "0.4 0.25" : undefined}
      />
      {/* habitacle fermé */}
      <path d="M-0.7 -0.5 L0.55 -0.45 Q1.05 0 0.55 0.45 L-0.7 0.5 Q-0.95 0 -0.7 -0.5 Z" fill={ghost ? "none" : "var(--bg-sunken)"} stroke={ghost ? body : "none"} strokeWidth={0.08} />
      {/* aileron arrière */}
      <rect x={-2.75} y={-0.95} width={0.32} height={1.9} fill={ghost ? "none" : line} stroke={ghost ? body : "none"} strokeWidth={0.08} />
      {/* feux arrière */}
      <rect x={-2.55} y={-0.8} width={0.16} height={0.42} fill={braking ? "var(--pedal-brake)" : "var(--surface-3)"} />
      <rect x={-2.55} y={0.38} width={0.16} height={0.42} fill={braking ? "var(--pedal-brake)" : "var(--surface-3)"} />
      {braking && <ellipse cx={-3.1} cy={0} rx={0.55} ry={1.1} fill="var(--pedal-brake)" opacity={0.45} />}
    </g>
  );
}

/**
 * Repères de freinage (F) et de remise des gaz (G) d'un tour : tiret en travers de sa trajectoire.
 * F vient de la comparaison de freinage d'apex-core (canal frein mesuré, jamais une décélération) :
 * absent quand le point manque ou est ambigu.
 */
interface Mark { x: number; y: number; nx: number; ny: number; kind: "brake" | "gas"; who: "ref" | "cmp"; corner: string; at: number }

function pointAt(xs: number[], ys: number[], m: number, step: number) {
  const n = xs.length;
  const i = Math.max(0, Math.min(n - 1, Math.round(m / step)));
  const a = Math.max(0, i - 4), b = Math.min(n - 1, i + 4);
  const dx = xs[b]! - xs[a]!, dy = ys[b]! - ys[a]!;
  const l = Math.hypot(dx, dy) || 1;
  return { x: xs[i]!, y: ys[i]!, nx: -dy / l, ny: dx / l };
}

function inputMarks(c: Comparison, geo: Geo): Mark[] {
  const out: Mark[] = [];
  const CX = geo.CX && geo.CY ? geo.CX : geo.X;
  const CY = geo.CX && geo.CY ? geo.CY : geo.Y;
  const put = (who: Mark["who"], kind: Mark["kind"], m: number | null | undefined, corner: string) => {
    if (m == null) return;
    out.push({ ...pointAt(who === "ref" ? geo.X : CX, who === "ref" ? geo.Y : CY, m, c.step_m), kind, who, corner, at: m });
  };
  for (const k of c.corners) {
    put("ref", "brake", k.braking?.reference?.start_m, k.corner.name);
    put("cmp", "brake", k.braking?.compared?.start_m, k.corner.name);
    put("ref", "gas", k.reference.throttle_on_m, k.corner.name);
    put("cmp", "gas", k.compared.throttle_on_m, k.corner.name);
  }
  return out;
}

/** Zones de freinage mesurées (début → relâchement) de chaque tour, en un tracé par tour. */
function brakeZonePaths(c: Comparison, geo: Geo): { ref: string; cmp: string } {
  const CX = geo.CX && geo.CY ? geo.CX : geo.X;
  const CY = geo.CX && geo.CY ? geo.CY : geo.Y;
  const seg = (xs: number[], ys: number[], z: { start_m: number; end_m: number } | null | undefined) => {
    if (!z) return "";
    const i0 = Math.max(0, Math.round(z.start_m / c.step_m)), i1 = Math.min(xs.length - 1, Math.round(z.end_m / c.step_m));
    return i1 > i0 ? linePath(xs.slice(i0, i1 + 1), ys.slice(i0, i1 + 1), 1) : "";
  };
  return {
    ref: c.corners.map((k) => seg(geo.X, geo.Y, k.braking?.reference)).join(""),
    cmp: c.corners.map((k) => seg(CX, CY, k.braking?.compared)).join(""),
  };
}

/** Jauges type télé : vitesse, rapport, gaz et frein des deux voitures. */
function Hud({ cmp, ci, ri, ghostMode, compact }: { cmp: Comparison; ci: number; ri: number; ghostMode: "time" | "distance"; compact?: boolean }) {
  const col = (label: string, color: string, tr: typeof cmp.ref_trace, i: number) => {
    const th = Math.max(0, Math.min(1, at(tr.throttle, i) ?? 0));
    const br = Math.max(0, Math.min(1, at(tr.brake, i) ?? 0));
    const g = at(tr.gear, i);
    return (
      <div className="hud-car">
        <div className="hud-name" style={{ borderColor: color }}>{label}</div>
        <div className="hud-row">
          <div className="hud-speed mono">{Math.round(at(tr.speed_kmh, i) ?? 0)}<small>km/h</small></div>
          {g != null && <div className="hud-gear mono">{Math.round(g) || "N"}</div>}
          <div className="hud-pedal" title="Frein"><i style={{ height: `${br * 100}%`, background: BRAKE }} /></div>
          <div className="hud-pedal" title="Gaz"><i style={{ height: `${th * 100}%`, background: GAS }} /></div>
        </div>
      </div>
    );
  };
  const gapS = at(cmp.delta_s, ci) ?? 0;
  const aheadM = (ri - ci) * cmp.step_m;
  return (
    <div className={`hud ${compact ? "compact" : ""}`} onPointerDown={(e) => e.stopPropagation()}>
      {col(cmp.reference.driver ?? `Réf. tour ${cmp.reference.number}`, REF_LINE, cmp.ref_trace, ri)}
      {col(`Toi, tour ${cmp.compared.number}`, CMP_LINE, cmp.cmp_trace, ci)}
      <div className={`hud-gap mono ${gapS > 0.005 ? "pos" : gapS < -0.005 ? "neg" : ""}`}>
        {gapS > 0 ? "+" : gapS < 0 ? "−" : "±"}{Math.abs(gapS).toFixed(2)} s
        <small>{ghostMode === "distance" ? "même endroit" : Math.abs(aheadM) < 1 ? "à hauteur" : `réf. ${Math.abs(aheadM).toFixed(0)} m ${aheadM > 0 ? "devant" : "derrière"}`}</small>
      </div>
    </div>
  );
}
const OFF_COLOR = "var(--danger)";

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

const SPEED_COLORS = [0, 1, 2, 3, 4, 5].map((k) => `var(--speed-${k})`);

/** Échelle du delta continu : 0,2 s gagnés ou perdus sur 100 m = couleur pleine. */
export const DELTA_FULL_S_PER_100M = 0.2;

/** Vitesse de variation de l'écart (s par mètre) autour de l'index `i`, sur ±15 m. */
export function deltaRate(c: Comparison, i: number): number {
  const w = Math.max(1, Math.round(15 / c.step_m));
  const a = Math.max(0, i - w), b = Math.min(c.delta_s.length - 1, i + w);
  return ((c.delta_s[b] ?? 0) - (c.delta_s[a] ?? 0)) / ((b - a) * c.step_m || 1);
}

/** Couleur du delta continu : vert gain, jaune perte, saturation selon l'ampleur (6 crans par côté). */
export function deltaColor(rate: number): string {
  const k = Math.max(-1, Math.min(1, (rate * 100) / DELTA_FULL_S_PER_100M));
  if (Math.abs(k) < 0.08) return "var(--surface-3)";
  const q = Math.ceil(Math.abs(k) * 6) / 6;
  return mix(k > 0 ? "loss" : "gain", 0.25 + 0.75 * q);
}

/** Couches fixes (ne dépendent pas du temps) : mémorisées. */
const StaticLayers = memo(function StaticLayers({ c, geo, mode, hasEdges = false, ribbonM = 12 }: { c: Comparison; geo: Geo; mode: ColorMode; hasEdges?: boolean; ribbonM?: number }) {
  const layers = useMemo(() => {
    const { X, Y, CX, CY } = geo;
    const step = Math.max(1, Math.round(4 / c.step_m));
    let band: [string, string][] = [];
    let cmpColored: [string, string][] | null = null;
    if (mode === "delta") {
      band = bucketPaths(X, Y, Math.max(1, Math.round(10 / c.step_m)), (i) => deltaColor(deltaRate(c, i)));
    } else if (mode === "studio") {
      band = [];
      if (CX && CY) {
        const t = c.cmp_trace;
        cmpColored = bucketPaths(CX, CY, step, (i) => {
          const b = t.brake[i] ?? 0;
          const th = t.throttle[i] ?? 0;
          if (b > 0.05) return "var(--pedal-brake)";
          if (th > 0.9) return "var(--pedal-full)";
          if (th > 0.05) return "var(--pedal-partial)";
          return "var(--pedal-coast)";
        });
      }
    } else {
      band = [];
      if (CX && CY) {
        const t = c.cmp_trace;
        cmpColored = bucketPaths(CX, CY, step, (i) => {
          if (mode === "inputs") {
            const b = t.brake[i] ?? 0;
            const th = t.throttle[i] ?? 0;
            if (b > 0.05) return b > 0.5 ? "var(--pedal-brake)" : "var(--pedal-brake-light)";
            if (th > 0.95) return "var(--pedal-full)";
            if (th > 0.05) return "var(--pedal-partial)";
            return "var(--pedal-coast)";
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

  if (mode === "studio") {
    // Piste sombre à sa largeur réelle (≈ 13 m autour de la trajectoire de référence), bords clairs ;
    // référence en pointillés blancs, ton tour coloré selon les pédales (vert gaz, rouge frein, blanc roue libre).
    return (
      <g>
        {!hasEdges && <path d={layers.refD} stroke="var(--map-asphalt-edge)" strokeWidth={15} strokeLinecap="round" strokeLinejoin="round" fill="none" />}
        {!hasEdges && <path d={layers.refD} stroke="var(--map-asphalt)" strokeWidth={13.4} strokeLinecap="round" strokeLinejoin="round" fill="none" />}
        <path d={layers.refD} stroke={mix("text", 0.85)} strokeWidth={1.6} strokeDasharray="2 4" fill="none" vectorEffect="non-scaling-stroke" />
        {layers.cmpColored?.map(([col, d]) => <path key={col} d={d} stroke={col} strokeWidth={2.6} strokeLinecap="round" fill="none" vectorEffect="non-scaling-stroke" />)}
        {layers.cmpOff && <path d={layers.cmpOff} stroke={OFF_COLOR} strokeWidth={6} strokeLinecap="round" fill="none" vectorEffect="non-scaling-stroke" />}
      </g>
    );
  }
  return (
    <g>
      {/* Ruban de piste : bords relevés quand ils existent (couche OverlayLayer), sinon largeur constante de 12 m. */}
      {!hasEdges && <path d={layers.refD} stroke="var(--map-asphalt-edge)" strokeWidth={ribbonM + 1.5} strokeLinecap="round" strokeLinejoin="round" fill="none" />}
      {!hasEdges && <path d={layers.refD} stroke="var(--map-asphalt)" strokeWidth={ribbonM} strokeLinecap="round" strokeLinejoin="round" fill="none" />}
      {layers.band.map(([col, d]) => (
        <path key={col} d={d} stroke={col} strokeWidth={mode === "delta" ? 11 : 6} strokeLinecap="round" fill="none" vectorEffect="non-scaling-stroke" />
      ))}
      <path d={layers.refD} stroke={REF_LINE} strokeWidth={1} opacity={mode === "delta" ? 0.6 : 1} fill="none" vectorEffect="non-scaling-stroke" />
      {layers.cmpColored
        ? layers.cmpColored.map(([col, d]) => <path key={col} d={d} stroke={col} strokeWidth={3} strokeLinecap="round" fill="none" vectorEffect="non-scaling-stroke" />)
        : layers.cmpD && <path d={layers.cmpD} stroke={CMP_LINE} strokeWidth={1.6} fill="none" vectorEffect="non-scaling-stroke" />}
      {layers.refOff && <path d={layers.refOff} stroke={OFF_COLOR} strokeWidth={5} strokeDasharray="4 4" strokeLinecap="round" fill="none" opacity={0.8} vectorEffect="non-scaling-stroke" />}
      {layers.cmpOff && <path d={layers.cmpOff} stroke={OFF_COLOR} strokeWidth={6} strokeLinecap="round" fill="none" vectorEffect="non-scaling-stroke" />}
    </g>
  );
});

/** Carrés de `size` m centrés sur les points, en un seul tracé. */
function squares(pts: [number, number][], size: number, keep?: (p: [number, number]) => boolean): string {
  const h = size / 2;
  let d = "";
  for (const p of pts) {
    if (keep && !keep(p)) continue;
    d += `M${(p[0] - h).toFixed(1)} ${(p[1] - h).toFixed(1)}h${size}v${size}h-${size}z`;
  }
  return d;
}

/**
 * Tronçons d'un bord de piste qui portent des points relevés (vibreur, ligne peinte) : chaque point
 * du bord est marqué s'il a un relevé à moins de `reach` m ; les trous d'un point sont comblés et les
 * tronçons de moins de `minPts` points ignorés (bruit). Rend une polyligne par tronçon.
 */
export function edgeRuns(edge: [number, number][], marks: [number, number][], reach = 4.5, minPts = 3): string {
  if (edge.length < 2 || marks.length === 0) return "";
  // Grille de 8 m sur les points du bord, pour trouver le segment le plus proche de chaque relevé.
  const cell = 8;
  const grid = new Map<string, number[]>();
  edge.forEach((p, i) => {
    const k = `${Math.floor(p[0] / cell)},${Math.floor(p[1] / cell)}`;
    const a = grid.get(k);
    if (a) a.push(i);
    else grid.set(k, [i]);
  });
  const segDist = (m: [number, number], a: [number, number], b: [number, number]) => {
    const dx = b[0] - a[0], dy = b[1] - a[1], l2 = dx * dx + dy * dy || 1;
    const t = Math.max(0, Math.min(1, ((m[0] - a[0]) * dx + (m[1] - a[1]) * dy) / l2));
    return Math.hypot(m[0] - a[0] - t * dx, m[1] - a[1] - t * dy);
  };
  const on = new Array<boolean>(edge.length).fill(false);
  for (const m of marks) {
    const cx = Math.floor(m[0] / cell), cy = Math.floor(m[1] / cell);
    let best = Infinity, bi = -1;
    for (let i = -1; i <= 1; i++) for (let j = -1; j <= 1; j++) {
      for (const k of grid.get(`${cx + i},${cy + j}`) ?? []) {
        for (const s of [k - 1, k]) {
          if (s < 0 || s + 1 >= edge.length) continue;
          const a = edge[s]!, b = edge[s + 1]!;
          if (Math.hypot(b[0] - a[0], b[1] - a[1]) > 15) continue;
          const d = segDist(m, a, b);
          if (d < best) (best = d), (bi = s);
        }
      }
    }
    if (bi >= 0 && best <= reach) on[bi] = on[bi + 1] = true;
  }
  for (let i = 1; i < on.length - 1; i++) if (!on[i] && on[i - 1] && on[i + 1]) on[i] = true;
  let d = "";
  let run: [number, number][] = [];
  const flush = () => {
    if (run.length >= minPts) d += run.map((p, k) => `${k ? "L" : "M"}${p[0].toFixed(1)} ${p[1].toFixed(1)}`).join("");
    run = [];
  };
  edge.forEach((p, i) => {
    // Un bord coupé (trou > 15 m) coupe aussi le tronçon.
    if (run.length && Math.hypot(p[0] - run[run.length - 1]![0], p[1] - run[run.length - 1]![1]) > 15) flush();
    if (on[i]) run.push(p);
    else flush();
  });
  flush();
  return d;
}

/** Polyligne coupée là où il manque des tranches (trous > 15 m). */
function brokenLine(pts: [number, number][]): string {
  let d = "";
  let prev: [number, number] | null = null;
  for (const p of pts) {
    const jump = !prev || Math.hypot(p[0] - prev[0], p[1] - prev[1]) > 15;
    d += `${jump ? "M" : "L"}${p[0].toFixed(1)} ${p[1].toFixed(1)}`;
    prev = p;
  }
  return d;
}

/**
 * Relevé de la piste : surface d'asphalte entre les bords, bords (limites de piste), vibreurs
 * rouge et blanc, bordures peintes et hors-piste, d'après la matière sous ta voiture.
 * Les points sont dans le repère des trajectoires (y inversé comme la carte).
 */
const OverlayLayer = memo(function OverlayLayer({ ov, studio, flip }: { ov: TrackOverlay; studio: boolean; flip: (p: [number, number]) => [number, number] }) {
  const d = useMemo(() => {
    const e = ov.edges;
    const L = e ? e.left.map(flip) : [];
    const R = e ? e.right.map(flip) : [];
    const ring = e ? `M${L.map((p) => `${p[0].toFixed(1)} ${p[1].toFixed(1)}`).join("L")}Z M${R.map((p) => `${p[0].toFixed(1)} ${p[1].toFixed(1)}`).join("L")}Z` : "";
    const kerbs = ov.kerbs.map(flip);
    const lines = ov.lines.map(flip);
    // Avec les bords : vibreurs et lignes dessinés le long du bord, en bandes continues.
    // Sans bords : repli sur les points relevés.
    const along = (pts: [number, number][]) => (e ? edgeRuns(L, pts) + edgeRuns(R, pts) : "");
    return {
      ring,
      left: e ? brokenLine(L) : "",
      right: e ? brokenLine(R) : "",
      kerbs: along(kerbs),
      kerbDots: e ? "" : squares(kerbs, 1.3),
      lines: e ? along(lines) : "",
      lineDots: e ? "" : squares(lines, 1.0),
      off: studio ? squares(ov.off.map(flip), 1.2) : "",
    };
  }, [ov, studio, flip]);
  return (
    <g style={{ pointerEvents: "none" }}>
      {d.ring && <path d={d.ring} fill="var(--map-asphalt)" fillRule="evenodd" />}
      {d.off && <path d={d.off} fill="var(--map-grass)" />}
      {d.left && <path d={d.left} stroke={mix("text", 0.45)} strokeWidth={1} fill="none" vectorEffect="non-scaling-stroke" />}
      {d.right && <path d={d.right} stroke={mix("text", 0.45)} strokeWidth={1} fill="none" vectorEffect="non-scaling-stroke" />}
      {d.lines && <path d={d.lines} stroke="var(--map-paint)" strokeWidth={0.6} strokeLinejoin="round" fill="none" />}
      {d.lineDots && <path d={d.lineDots} fill="var(--map-paint)" />}
      {/* Vibreur : bande de 2,4 m au bord, rouge et blanc par pavés de 2 m, comme en vrai. */}
      {d.kerbs && <path d={d.kerbs} stroke="var(--kerb-red)" strokeWidth={2.4} strokeLinejoin="round" fill="none" />}
      {d.kerbs && <path d={d.kerbs} stroke="var(--kerb-white)" strokeWidth={2.4} strokeDasharray="2 2" strokeLinejoin="round" fill="none" />}
      {d.kerbDots && <path d={d.kerbDots} fill="var(--kerb-red)" />}
    </g>
  );
});

const flipY = (p: [number, number]): [number, number] => [p[0], -p[1]];

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
    // Numéro d'ordre sur le tour : les chiffres d'un nom (« Lesmo 1 ») ne sont pas des numéros de virage.
    return { id: k.corner.id, num: String(num + 1), name: k.corner.name, x: X[i]!, y: Y[i]!, nx, ny, loss: k.time_delta_s };
  });
}

interface View { cx: number; cy: number; w: number }

/** Largeur estimée d'un nom de virage en Saira 14 px (6,9 px par caractère en moyenne, mesuré). */
const textW = (t: string) => Math.ceil(t.length * 6.9) + 4;

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
      <path d={d} stroke="var(--text-2)" strokeWidth={2} fill="none" vectorEffect="non-scaling-stroke" />
      <circle cx={x} cy={y} r={s * 0.035} fill={CMP_LINE} stroke="var(--text)" strokeWidth={1.5} vectorEffect="non-scaling-stroke" />
    </svg>
  );
});

export function TrackMap({ cmp, store, selected, onSelectCorner, height = 420, size = "normal", onSize, studio = false, overlay = null, simple = false }: {
  /** Vue résumé : outils repliés derrière un bouton, la carte gain / perte seule. */
  simple?: boolean;
  /** Relevé de la piste (vibreurs, bords) d'après tes tours. */
  overlay?: TrackOverlay | null;
  cmp: Comparison;
  store: ReplayStore;
  selected: number | null;
  onSelectCorner: (id: number) => void;
  height?: number;
  size?: MapSize;
  onSize?: (s: MapSize) => void;
  /** Relecture plein écran : piste sombre, lignes et pédales seulement, caméra embarquée. */
  studio?: boolean;
}) {
  const geo = useMemo(() => buildGeo(cmp), [cmp]);
  const labels = useMemo(() => (geo ? cornerLabels(cmp, geo) : []), [cmp, geo]);
  const host = useRef<HTMLDivElement>(null);
  const [px, setPx] = useState(400);
  const [mode, setMode] = useState<ColorMode>(studio ? "studio" : "delta");
  const [tools, setTools] = useState(!simple);
  const [follow, setFollow] = useState(studio);
  /** Caméra inclinée « poursuite » (relecture plein écran). */
  const [tilt, setTilt] = useState(false);
  const [marks, setMarks] = useState(true);
  const [showSurf, setShowSurf] = useState(true);
  const inputs = useMemo(() => (geo ? inputMarks(cmp, geo) : []), [cmp, geo]);
  const zones = useMemo(() => (geo ? brakeZonePaths(cmp, geo) : { ref: "", cmp: "" }), [cmp, geo]);
  const priority = useMemo(() => priorityCorners(cmp), [cmp]);
  /** Orientation en mode suivi : la voiture pointe toujours vers le haut. */
  const [carUp, setCarUp] = useState(true);
  /** Échelle du suivi rapproché (mètres par pixel), indépendante de la taille de la carte. */
  const [zoomMpp, setZoomMpp] = useState(studio ? 0.12 : 0.22);
  const zoomW = zoomMpp * px;
  const full = useMemo<View | null>(() => {
    if (!geo) return null;
    const b = geo.bounds;
    const w = Math.max(b.x1 - b.x0, (b.y1 - b.y0) * (px / height)) * 1.18 + 60 * ((b.x1 - b.x0) / px);
    return { cx: (b.x0 + b.x1) / 2, cy: (b.y0 + b.y1) / 2, w };
  }, [geo, px, height]);
  const [view, setView] = useState<View | null>(null);
  useEffect(() => {
    setView(null);
  }, [cmp]);

  // Le conteneur n'existe qu'avec une carte : l'observateur est (ré)attaché quand elle apparaît.
  const hasMap = geo != null;
  useEffect(() => {
    const el = host.current;
    if (!el) return;
    setPx(el.clientWidth || 400);
    const ro = new ResizeObserver(() => setPx(el.clientWidth || 400));
    ro.observe(el);
    return () => ro.disconnect();
  }, [hasMap]);

  // Un virage choisi ailleurs (liste des virages, fiche) : la carte zoome dessus.
  const lastSel = useRef<number | null>(null);
  useEffect(() => {
    if (selected == null || selected === lastSel.current) {
      lastSel.current = selected;
      return;
    }
    lastSel.current = selected;
    const k = labels.find((x) => x.id === selected);
    if (k) {
      setFollow(false);
      setView({ cx: k.x, cy: k.y, w: 380 });
    }
  }, [selected, labels]);

  const rs = useReplay(store);
  const { ci, ri } = positionsAt(cmp, rs.t, rs.ghost);

  // Lecture lancée : la caméra passe automatiquement en suivi rapproché.
  const wasPlaying = useRef(false);
  useEffect(() => {
    if (rs.playing && !wasPlaying.current) setFollow(true);
    wasPlaying.current = rs.playing;
  }, [rs.playing]);

  const CX = geo?.CX && geo.CY ? geo.CX : geo?.X;
  const CY = geo?.CX && geo.CY ? geo.CY : geo?.Y;
  const car = geo && CX && CY ? { x: at(CX, ci)!, y: at(CY, ci)!, h: headingAt(CX, CY, ci, 4 / cmp.step_m) } : null;
  const ghost = geo ? { x: at(geo.X, ri)!, y: at(geo.Y, ri)!, h: headingAt(geo.X, geo.Y, ri, 4 / cmp.step_m) } : null;

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
  // Écouteur de molette posé une fois : la vue change à chaque image pendant la lecture.
  const zoomRef = useRef(zoomBy);
  zoomRef.current = zoomBy;
  useEffect(() => {
    const el = host.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      zoomRef.current(Math.exp(e.deltaY * 0.0015), e.clientX, e.clientY);
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, [hasMap]);

  if (!geo || !v) {
    return <div className="card empty" style={{ padding: 30 }}>Carte non disponible : la source ne fournit pas la position du véhicule.</div>;
  }

  const hView = v.w * (height / px);
  const vb = `${v.cx - v.w / 2} ${v.cy - hView / 2} ${v.w} ${hView}`;
  const zoomTo = (x: number, y: number, w: number) => { setFollow(false); setView({ cx: x, cy: y, w }); };
  const g = `rotate(${rot.toFixed(2)} ${v.cx} ${v.cy})`;
  // Repère monde → pixels de la carte (rotation de la vue pilote comprise) : les étiquettes sont posées
  // dans un calque en pixels, toujours droites et à taille fixe.
  const cosR = Math.cos((rot * Math.PI) / 180), sinR = Math.sin((rot * Math.PI) / 180);
  const toScreen = (x: number, y: number): [number, number] => {
    const dx = x - v.cx, dy = y - v.cy;
    return [(dx * cosR - dy * sinR) / mPerPx + px / 2, (dx * sinR + dy * cosR) / mPerPx + height / 2];
  };
  const ribbonM = Math.max(12, 9 * mPerPx);

  // Étiquettes de virage : numéro d'ordre, nom quand la place le permet, jamais l'une sur l'autre.
  const obstacles: Box[] = [{ x: px - 48, y: height - (onSize ? 128 : 88), w: 48, h: onSize ? 128 : 88 }];
  if (zoomed) obstacles.push({ x: 0, y: height - 128, w: 128, h: 128 });
  if (studio) obstacles.push({ x: 0, y: 0, w: Math.min(px, 560), h: 52 });
  /** Trajectoire à l'écran, en petits carrés tous les ~12 px : les étiquettes l'évitent si elles peuvent. */
  function trackBoxes(): Box[] {
    const out: Box[] = [];
    // Carrés de la largeur du ruban (+ repères F / G), espacés de la moitié de leur côté.
    const side = Math.max(12, ribbonM / mPerPx + 10);
    const stride = Math.max(1, Math.round((side * 0.5 * mPerPx) / cmp.step_m));
    for (let i = 0; i < geo!.X.length; i += stride) {
      const [x, y] = toScreen(geo!.X[i]!, geo!.Y[i]!);
      if (x > -side && y > -side && x < px + side && y < height + side) out.push({ x: x - side / 2, y: y - side / 2, w: side, h: side });
    }
    return out;
  }
  const placed = tilt && follow ? [] : placeLabels(
    labels.map((k) => {
      const [sx, sy] = toScreen(k.x, k.y);
      const n = [k.nx * cosR - k.ny * sinR, k.nx * sinR + k.ny * cosR];
      const prio = priority.findIndex((p) => p.corner.id === k.id);
      const extra = prio >= 0 ? 8 + textW(lossText(k.loss)) : 0;
      return { id: k.id, x: sx, y: sy, nx: n[0]!, ny: n[1]!, small: { w: 26 + extra, h: 26 }, big: { w: 26 + 8 + textW(k.name) + extra, h: 26 }, priority: (selected === k.id ? 100 : 0) + (prio >= 0 ? 50 - prio : 0) + Math.max(0, k.loss) };
    }).filter((k) => k.x > -40 && k.y > -40 && k.x < px + 40 && k.y < height + 40),
    px, height, obstacles, 3, trackBoxes(),
  );
  const byId = new Map(labels.map((k) => [k.id, k]));

  // Écart de point de freinage, écrit près de TON repère F : virage choisi toujours, virages
  // prioritaires dès qu'on est assez près. Jamais sans les deux points mesurés.
  const gapChips = (() => {
    if (!marks || (tilt && follow)) return [];
    const CXs = geo.CX && geo.CY ? geo.CX : geo.X, CYs = geo.CX && geo.CY ? geo.CY : geo.Y;
    const want = cmp.corners.filter((k) => k.corner.id === selected || (v.w < 1600 && priority.some((p) => p.corner.id === k.corner.id)));
    const items = want.flatMap((k) => {
      const t = brakeGapText(k.braking);
      const z = k.braking.compared;
      if (!t || !z) return [];
      const p = pointAt(CXs, CYs, z.start_m, cmp.step_m);
      const [sx, sy] = toScreen(p.x, p.y);
      const n = [p.nx * cosR - p.ny * sinR, p.nx * sinR + p.ny * cosR];
      const text = `F ${t}`;
      const w = textW(text) + 12;
      return [{ id: k.corner.id, text, item: { id: k.corner.id, x: sx, y: sy, nx: -n[0]!, ny: -n[1]!, small: { w, h: 24 }, big: { w, h: 24 }, priority: k.corner.id === selected ? 10 : 1 } }];
    });
    const out = placeLabels(items.map((i) => i.item), px, height, [...obstacles, ...placed], 3, trackBoxes());
    return out.map((o) => ({ ...o, text: items.find((i) => i.id === o.id)!.text, ax: items.find((i) => i.id === o.id)!.item.x, ay: items.find((i) => i.id === o.id)!.item.y }));
  })();

  // Repères de distance tous les 100 m (traits discrets), cote tous les 500 m quand on est assez près.
  const ticks = (() => {
    const n = geo.X.length, each = Math.round(100 / cmp.step_m);
    let d = "";
    const texts: { x: number; y: number; t: string }[] = [];
    const half = ribbonM / 2 + 2 * mPerPx, len = 5 * mPerPx;
    for (let i = each; i < n - each / 2; i += each) {
      const a = Math.max(0, i - 3), b = Math.min(n - 1, i + 3);
      const dx = geo.X[b]! - geo.X[a]!, dy = geo.Y[b]! - geo.Y[a]!;
      const l = Math.hypot(dx, dy) || 1;
      const nx = -dy / l, ny = dx / l;
      const x0 = geo.X[i]! + nx * half, y0 = geo.Y[i]! + ny * half;
      d += `M${x0.toFixed(1)} ${y0.toFixed(1)}L${(x0 + nx * len).toFixed(1)} ${(y0 + ny * len).toFixed(1)}`;
      const m = Math.round(i * cmp.step_m);
      if (m % 500 === 0 && v.w < 1600) {
        const [sx, sy] = toScreen(x0 + nx * (len + 9 * mPerPx), y0 + ny * (len + 9 * mPerPx));
        texts.push({ x: sx, y: sy, t: m % 1000 === 0 ? `${m / 1000} km` : `${m} m` });
      }
    }
    // Une cote n'est écrite que si elle ne touche aucune étiquette de virage.
    return { d, texts: texts.filter((t) => !placed.some((b) => overlaps({ x: t.x - 22, y: t.y - 9, w: 44, h: 18 }, b, 2))) };
  })();

  // Départ : trait en travers de la piste ; sens : chevron 120 m après la ligne.
  const startMark = (() => {
    const X = geo.X, Y = geo.Y;
    const dir = (i: number) => {
      const a = Math.max(0, i - 3), b = Math.min(X.length - 1, i + 3);
      const dx = X[b]! - X[a]!, dy = Y[b]! - Y[a]!;
      const l = Math.hypot(dx, dy) || 1;
      return [dx / l, dy / l] as const;
    };
    const [tx, ty] = dir(0);
    const s0 = ribbonM / 2 + 3 * mPerPx;
    const line = { x1: X[0]! - ty * s0, y1: Y[0]! + tx * s0, x2: X[0]! + ty * s0, y2: Y[0]! - tx * s0 };
    const k = Math.min(X.length - 1, Math.round(120 / cmp.step_m));
    const [ux, uy] = dir(k);
    const L = 9 * mPerPx, W = 6 * mPerPx, cx = X[k]!, cy = Y[k]!;
    const arrow = `M${cx + ux * L} ${cy + uy * L}L${cx - ux * L * 0.4 - uy * W} ${cy - uy * L * 0.4 + ux * W}L${cx - ux * L * 0.1} ${cy - uy * L * 0.1}L${cx - ux * L * 0.4 + uy * W} ${cy - uy * L * 0.4 - ux * W}Z`;
    return { line, arrow };
  })();

  const selectCorner = (id: number) => {
    const k = byId.get(id);
    onSelectCorner(id);
    if (k) zoomTo(k.x, k.y, 380);
  };
  /** Virage suivant / précédent au clavier (flèches gauche / droite sur la carte). */
  const step = (dir: 1 | -1) => {
    if (!labels.length) return;
    const i = labels.findIndex((k) => k.id === selected);
    const next = labels[(i < 0 ? (dir > 0 ? 0 : labels.length - 1) : (i + dir + labels.length) % labels.length)]!;
    selectCorner(next.id);
  };

  return (
    <div className={`trackmap ${studio ? "trackmap-studio" : ""}`}>
      {!studio && simple && !tools && (
        <div className="trackmap-tools">
          <button className="btn ghost small" onClick={() => setTools(true)}>Options de la carte</button>
        </div>
      )}
      {!studio && tools && (
        <div className="trackmap-tools">
          <label className="tm-mode">
            Couleur
            <select value={mode} onChange={(e) => setMode(e.target.value as ColorMode)} aria-label="Couleur du tracé">
              <option value="delta">Écart de temps</option>
              <option value="inputs">Pédales</option>
              <option value="speed">Vitesse</option>
            </select>
          </label>
          <div className="seg">
            <button className={marks ? "on" : ""} onClick={() => setMarks((m) => !m)} title="Points de freinage (F) et de remise des gaz (G), les tiens et ceux de la référence">Repères F / G</button>
            {overlay && (overlay.edges || overlay.kerbs.length > 0) && <button className={showSurf ? "on" : ""} onClick={() => setShowSurf((m) => !m)} title={`Bords de piste et vibreurs relevés sur ${overlay.laps} tours`}>Bords</button>}
            <button className={follow ? "on" : ""} onClick={() => setFollow((f) => !f)} title="La caméra suit la voiture (molette : zoom). Activé automatiquement en lecture.">Suivre</button>
            <button onClick={() => { setFollow(false); setView(null); }} title="Voir tout le circuit">Circuit</button>
          </div>
          <details className="tm-more">
            <summary>Plus</summary>
            <div className="tm-more-body">
              <div className="seg" title="Où placer la voiture de la référence">
                <span className="seg-label">Réf.</span>
                <button className={rs.ghost === "time" ? "on" : ""} onClick={() => store.setGhost("time")} title="Où était la référence au même instant : elle se détache devant ou derrière toi">Même temps</button>
                <button className={rs.ghost === "distance" ? "on" : ""} onClick={() => store.setGhost("distance")} title="La référence roule à ta hauteur : compare freinages et remises des gaz au même endroit">Même endroit</button>
              </div>
              {follow && <button className={`btn small ${carUp ? "on" : ""}`} onClick={() => setCarUp((x) => !x)} title="La carte tourne pour que la voiture pointe vers le haut">Vue pilote</button>}
              {onSize && (
                <div className="seg" title="Taille de la carte (Échap : quitter le plein écran)">
                  <button className={size === "normal" ? "on" : ""} onClick={() => onSize("normal")}>Normal</button>
                  <button className={size === "half" ? "on" : ""} onClick={() => onSize("half")}>50 %</button>
                  <button className={size === "full" ? "on" : ""} onClick={() => onSize("full")}>Relecture plein écran</button>
                </div>
              )}
            </div>
          </details>
        </div>
      )}
      <div
        ref={host}
        className="trackmap-canvas"
        style={{ height }}
        tabIndex={0}
        role="application"
        aria-label="Carte du circuit : flèches gauche et droite pour passer d'un virage à l'autre, plus et moins pour zoomer"
        onKeyDown={(e) => {
          if (e.key === "ArrowRight") (e.preventDefault(), step(1));
          else if (e.key === "ArrowLeft") (e.preventDefault(), step(-1));
          else if (e.key === "+" || e.key === "=") zoomBy(0.7);
          else if (e.key === "-") zoomBy(1.4);
          else if (e.key === "0") (setFollow(false), setView(null));
        }}
        onPointerDown={(e) => { if (!(tilt && follow)) onDown(e); }}
        onPointerMove={onMove}
        onPointerUp={onUp}
        onPointerCancel={onUp}
        onDoubleClick={() => !studio && onSize?.(size === "full" ? "normal" : "full")}
      >
        <svg
          viewBox={vb}
          width="100%"
          height={height}
          preserveAspectRatio="xMidYMid meet"
          style={tilt && follow ? { transform: "perspective(900px) rotateX(42deg) scale(1.35)", transformOrigin: "50% 62%" } : undefined}
        >
          <g transform={g}>
            {overlay && showSurf && <OverlayLayer ov={overlay} studio={studio} flip={flipY} />}
            <StaticLayers c={cmp} geo={geo} mode={mode} hasEdges={!!overlay?.edges && showSurf} ribbonM={ribbonM} />
            <path d={ticks.d} stroke="var(--text-2)" strokeWidth={1} fill="none" vectorEffect="non-scaling-stroke" opacity={0.45} />
            <line {...startMark.line} stroke="var(--text)" strokeWidth={4} vectorEffect="non-scaling-stroke" />
            <line {...startMark.line} stroke="var(--bg-sunken)" strokeWidth={4} strokeDasharray="3 3" vectorEffect="non-scaling-stroke" />
            <path d={startMark.arrow} fill="var(--text-2)" />
            {marks && (
              <g style={{ pointerEvents: "none" }}>
                {/* Zones de freinage mesurées : rouge, la tienne pleine, celle de la référence plus fine. */}
                <path d={zones.ref} stroke="var(--pedal-brake)" strokeWidth={3} opacity={0.55} strokeLinecap="round" fill="none" vectorEffect="non-scaling-stroke" />
                <path d={zones.cmp} stroke="var(--pedal-brake)" strokeWidth={5} opacity={0.8} strokeLinecap="round" fill="none" vectorEffect="non-scaling-stroke" />
              </g>
            )}
            {marks && (() => {
              // Tirets en travers de la piste : rouge = début du freinage, vert = remise des gaz ;
              // trait plein = toi, contour bleu = référence. Lettre F / G quand on est assez près.
              const half = 9 * mPerPx;
              const showText = v.w < 900;
              return inputs.map((m, i) => {
                const col = m.kind === "brake" ? BRAKE : GAS;
                const side = m.who === "ref" ? 1 : -1;
                const x1 = m.x - m.nx * half, y1 = m.y - m.ny * half, x2 = m.x + m.nx * half, y2 = m.y + m.ny * half;
                const tx = m.x + m.nx * side * (half + 9 * mPerPx), ty = m.y + m.ny * side * (half + 9 * mPerPx);
                return (
                  <g key={i}>
                    <title>{`${m.who === "ref" ? "Référence" : "Toi"} : ${m.kind === "brake" ? "freinage" : "remise des gaz"} (${m.corner}, ${m.at.toFixed(0)} m)`}</title>
                    {m.who === "ref" ? (
                      <>
                        <line x1={x1} y1={y1} x2={x2} y2={y2} stroke={REF_LINE} strokeWidth={7} strokeLinecap="round" vectorEffect="non-scaling-stroke" />
                        <line x1={x1} y1={y1} x2={x2} y2={y2} stroke={col} strokeWidth={3} strokeLinecap="round" vectorEffect="non-scaling-stroke" />
                      </>
                    ) : (
                      <>
                        <line x1={x1} y1={y1} x2={x2} y2={y2} stroke="var(--text)" strokeWidth={7} strokeLinecap="round" vectorEffect="non-scaling-stroke" />
                        <line x1={x1} y1={y1} x2={x2} y2={y2} stroke={col} strokeWidth={4} strokeLinecap="round" vectorEffect="non-scaling-stroke" />
                      </>
                    )}
                    {showText && (
                      <g transform={`rotate(${(-rot).toFixed(2)} ${tx} ${ty})`}>
                        <text x={tx} y={ty + 4 * mPerPx} textAnchor="middle" fontSize={12 * mPerPx} fontWeight={900} fill={m.who === "ref" ? REF_LINE : "var(--text)"} stroke="var(--bg-sunken)" strokeWidth={3 * mPerPx} paintOrder="stroke" fontFamily="var(--font-ui)">
                          {m.kind === "brake" ? "F" : "G"}
                        </text>
                      </g>
                    )}
                  </g>
                );
              });
            })()}
            {(() => {
              // Voitures à l'échelle réelle quand on est près, jamais plus petites que 24 px à l'écran.
              const sc = Math.max(studio ? 1.6 : 1, ((follow ? (studio ? 30 : 54) : 32) * mPerPx) / 5);
              // « Même endroit » : les deux voitures roulent côte à côte (décalées de part et d'autre de la
              // trajectoire) pour qu'on voie qui freine ou remet les gaz en premier.
              const side = (p: { x: number; y: number; h: number }, k: number) => {
                if (rs.ghost !== "distance") return p;
                const a = (p.h * Math.PI) / 180;
                const o = k * 1.15 * sc;
                return { ...p, x: p.x - Math.sin(a) * o, y: p.y + Math.cos(a) * o };
              };
              const g2 = ghost && side(ghost, -1);
              const c2 = car && side(car, 1);
              return (
                <>
                  {g2 && <Car x={g2.x} y={g2.y} heading={g2.h} scale={sc} body={REF_LINE} ghost braking={(at(cmp.ref_trace.brake, ri) ?? 0) > 0.05} />}
                  {c2 && <Car x={c2.x} y={c2.y} heading={c2.h} scale={sc} body={CMP_LINE} braking={(at(cmp.cmp_trace.brake, ci) ?? 0) > 0.05} />}
                </>
              );
            })()}
          </g>
        </svg>
        <svg className="trackmap-labels" width={px} height={height} aria-hidden={false}>
          {ticks.texts.map((t) => (
            <text key={t.t} x={t.x} y={t.y + 4} textAnchor="middle" fontSize={12} fill="var(--text-2)" stroke="var(--bg-sunken)" strokeWidth={3} paintOrder="stroke">{t.t}</text>
          ))}
          {gapChips.map((c) => (
            <g key={`gap-${c.id}`} data-label-group className="tm-gap">
              <line x1={c.ax} y1={c.ay} x2={c.x + c.w / 2} y2={c.y + c.h / 2} stroke="var(--pedal-brake)" strokeWidth={1} />
              <rect x={c.x} y={c.y} width={c.w} height={c.h} rx={4} fill="var(--overlay)" stroke="var(--pedal-brake)" strokeWidth={1} />
              <text x={c.x + 6} y={c.y + 17} fontSize={14} fontWeight={700} fill="var(--text)">{c.text}</text>
            </g>
          ))}
          {placed.map((l) => {
            const k = byId.get(l.id)!;
            const [sx, sy] = toScreen(k.x, k.y);
            const sel = selected === l.id;
            const r = l.h / 2;
            const nameLeft = (l.named || l.w > l.h + 1) && l.x + r < l.cx - 1;
            const isPrio = priority.some((p) => p.corner.id === l.id);
            return (
              <g key={l.id} data-label-group className="tm-label" role="button" tabIndex={-1} aria-label={`Virage ${k.num}, ${k.name}${isPrio ? `, priorité : ${lossText(k.loss)}` : ""}`} onPointerDown={(e) => e.stopPropagation()} onClick={() => selectCorner(l.id)}>
                <title>{`${k.name} : ${k.loss >= 0 ? "+" : "−"}${Math.abs(k.loss).toFixed(2)} s sur la référence`}</title>
                <line x1={sx} y1={sy} x2={l.cx} y2={l.cy} stroke="var(--text-2)" strokeWidth={1} />
                {(l.named || isPrio) && <rect x={l.x} y={l.y} width={l.w} height={l.h} rx={r} fill="var(--overlay)" stroke={sel ? "var(--text)" : isPrio ? "var(--accent)" : "var(--line-strong)"} strokeWidth={isPrio ? 1.5 : 1} />}
                <circle cx={l.cx} cy={l.cy} r={r - 1} fill={sel ? "var(--text)" : "var(--surface-2)"} stroke={isPrio ? "var(--accent)" : "var(--text)"} strokeWidth={isPrio ? 2.5 : 1.5} />
                <text x={l.cx} y={l.cy + 5} textAnchor="middle" fontSize={14} fontWeight={800} fill={sel ? "var(--bg-sunken)" : "var(--text)"}>{k.num}</text>
                <text x={nameLeft ? l.cx - r - 6 : l.cx + r + 6} y={l.cy + 5} textAnchor={nameLeft ? "end" : "start"} fontSize={14} fontWeight={600} fill="var(--text)">
                  {l.named && k.name}
                  {isPrio && <tspan fill="var(--loss)" fontWeight={800}>{l.named ? "  " : ""}{lossText(k.loss)}</tspan>}
                </text>
              </g>
            );
          })}
        </svg>
        {zoomed && car && <Overview geo={geo} x={car.x} y={car.y} />}
        {studio && (
          <div className="studio-maptools" onPointerDown={(e) => e.stopPropagation()}>
            <button className={tilt && follow ? "on" : ""} onClick={() => { setFollow(true); setCarUp(true); setTilt((x) => !x); }} title="Caméra inclinée derrière la voiture">3D</button>
            <button className={follow && !tilt ? "on" : ""} onClick={() => { setFollow(true); setCarUp(true); setTilt(false); }} title="Vue du dessus qui suit la voiture">Dessus</button>
            <button className={!follow ? "on" : ""} onClick={() => { setFollow(false); setTilt(false); setView(null); }} title="Tout le circuit">Circuit</button>
            <button className={marks ? "on" : ""} onClick={() => setMarks((m) => !m)} title="Points de freinage (F) et de remise des gaz (G)">F / G</button>
            {overlay && <button className={showSurf ? "on" : ""} onClick={() => setShowSurf((m) => !m)} title={`Bords de piste et vibreurs relevés sur ${overlay.laps} tours`}>Piste</button>}
            {overlay && showSurf && (
              <span className="studio-source">
                {overlay.edges?.source === "tum" ? "Bords : largeur mesurée + tes tours" : overlay.edges ? `Bords estimés sur ${overlay.laps} tours` : "Pas encore assez de tours pour les bords"}
                {overlay.kerbs.length + overlay.lines.length === 0 ? " · vibreurs relevés dès tes prochaines sessions" : ""}
              </span>
            )}
          </div>
        )}
        <div className="trackmap-zoom">
          {onSize && (
            <button
              onPointerDown={(e) => e.stopPropagation()}
              onClick={() => onSize(size === "full" ? "normal" : "full")}
              aria-label={size === "full" ? "Quitter le plein écran" : "Relecture plein écran"}
              title={size === "full" ? "Quitter le plein écran (Échap)" : "Plein écran (double-clic sur la carte)"}
            >
              {size === "full" ? "⤡" : "⤢"}
            </button>
          )}
          <button onPointerDown={(e) => e.stopPropagation()} onClick={() => zoomBy(0.7)} aria-label="Zoomer">+</button>
          <button onPointerDown={(e) => e.stopPropagation()} onClick={() => zoomBy(1.4)} aria-label="Dézoomer">−</button>
        </div>
      </div>
      {!studio && (
        <div className="trackmap-hud-row">
          <Hud cmp={cmp} ci={ci} ri={ri} ghostMode={rs.ghost} compact={px < 560} />
          {px >= 560 && <div className="hud-clock mono">{lapTime(rs.t - store.start)}</div>}
        </div>
      )}
      {!studio && (
        <div className="trackmap-legend">
          {mode === "delta" && (
            <span className="tm-scale" title="Couleur selon le temps gagné ou perdu sur la référence, mètre après mètre">
              <b>Gain</b>
              <i className="tm-scale-bar" />
              <b>Perte</b>
              <small>couleur pleine : {String(DELTA_FULL_S_PER_100M).replace(".", ",")} s par 100 m</small>
            </span>
          )}
          {mode === "inputs" && <span><i style={{ background: "var(--pedal-brake)" }} />frein <i style={{ background: "var(--pedal-partial)", marginLeft: 8 }} />gaz partiel <i style={{ background: "var(--pedal-full)", marginLeft: 8 }} />plein gaz <i style={{ background: "var(--pedal-coast)", marginLeft: 8 }} />roue libre</span>}
          {mode === "speed" && <span>{SPEED_COLORS.map((col, i) => <i key={col} style={{ background: col, marginLeft: i ? 2 : 0 }} />)} lent → rapide</span>}
          <span><i style={{ background: REF_LINE }} />Référence : {cmp.reference.driver ?? `tour ${cmp.reference.number}`} ({lapTime(cmp.reference.lap_time_s)}), voiture en pointillé</span>
          <span><i style={{ background: CMP_LINE }} />Toi : tour {cmp.compared.number} ({lapTime(cmp.compared.lap_time_s)})</span>
          {(cmp.cmp_off_track_m?.length > 0 || cmp.ref_off_track_m?.length > 0) && <span><i style={{ background: OFF_COLOR }} />hors piste</span>}
          {priority.length > 0 && <span><i style={{ background: "var(--accent)" }} />cerclé orange : les {priority.length} virage{priority.length > 1 ? "s" : ""} où tu perds le plus (plus de 0,05 s)</span>}
          {marks && <span><i style={{ background: BRAKE }} />F début du freinage (pédale à 10 %), zone rouge : freinage <i style={{ background: GAS, marginLeft: 8 }} />G gaz</span>}
          <span>Traits : tous les 100 m ; flèche : sens de la piste</span>
        </div>
      )}
    </div>
  );
}
