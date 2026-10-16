import { useMemo } from "react";
import type { Comparison } from "../lib/types";

/**
 * Carte du circuit : trajectoire de référence colorée selon le gain/la perte locale
 * (dérivée du delta sur ~20 m). Rouge = perte, vert = gain.
 * Affiche « non disponible » si la source ne fournit pas de position X/Y.
 */
export function TrackMap({ cmp, cursor, selected, onSelectCorner }: {
  cmp: Comparison;
  cursor: number | null;
  selected: number | null;
  onSelectCorner: (id: number) => void;
}) {
  const r = cmp.ref_trace;
  const geo = useMemo(() => {
    if (!r.x || !r.y) return null;
    const xs = r.x;
    const ys = r.y;
    const minX = Math.min(...xs), maxX = Math.max(...xs), minY = Math.min(...ys), maxY = Math.max(...ys);
    const span = Math.max(maxX - minX, maxY - minY) || 1;
    const size = 1000, pad = 40;
    const px = (i: number) => pad + ((xs[i]! - minX) / span) * (size - 2 * pad);
    const py = (i: number) => size - pad - ((ys[i]! - minY) / span) * (size - 2 * pad);
    const step = Math.max(1, Math.round(20 / cmp.step_m));
    const segs: { d: string; color: string }[] = [];
    for (let i = 0; i + step < xs.length; i += step) {
      const rate = ((cmp.delta_s[i + step] ?? 0) - (cmp.delta_s[i] ?? 0)) / (step * cmp.step_m); // s/m
      const k = Math.max(-1, Math.min(1, rate * 400)); // ±2,5 ms/m = saturation
      const color = k > 0.05 ? `rgba(255,77,79,${0.35 + 0.65 * k})` : k < -0.05 ? `rgba(46,204,113,${0.35 - 0.65 * k})` : "#3a4456";
      segs.push({ d: `M${px(i).toFixed(1)},${py(i).toFixed(1)} L${px(i + step).toFixed(1)},${py(i + step).toFixed(1)}`, color });
    }
    const corners = cmp.corners.map((c) => {
      const i = Math.min(Math.round(c.corner.apex_m / cmp.step_m), xs.length - 1);
      return { id: c.corner.id, name: c.corner.name, x: px(i), y: py(i), loss: c.time_delta_s };
    });
    return { segs, corners, px, py, size };
  }, [r, cmp]);

  if (!geo) {
    return <div className="card empty" style={{ padding: 30 }}>Carte non disponible : la source ne fournit pas la position du véhicule.</div>;
  }
  return (
    <svg viewBox={`0 0 ${geo.size} ${geo.size}`} style={{ width: "100%", background: "var(--panel)", borderRadius: 10, border: "1px solid var(--line)" }}>
      {geo.segs.map((s, i) => (
        <path key={i} d={s.d} stroke={s.color} strokeWidth={9} strokeLinecap="round" fill="none" />
      ))}
      {geo.corners.map((c) => (
        <g key={c.id} onClick={() => onSelectCorner(c.id)} style={{ cursor: "pointer" }}>
          <circle cx={c.x} cy={c.y} r={selected === c.id ? 24 : 18} fill={selected === c.id ? "#ff4d2e" : "#12161d"} stroke="#e8ecf2" strokeWidth={2} />
          <text x={c.x} y={c.y + 6} textAnchor="middle" fontSize={17} fill="#e8ecf2" fontFamily="monospace">{c.name.replace("V", "")}</text>
        </g>
      ))}
      {cursor != null && <circle cx={geo.px(cursor)} cy={geo.py(cursor)} r={11} fill="#fff" stroke="#000" strokeWidth={3} />}
    </svg>
  );
}
