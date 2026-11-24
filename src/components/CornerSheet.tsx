import { useEffect, useRef, useState } from "react";
import { brakeGapText, lossText, minSpeedText, throttleGapText } from "../lib/braking";
import { PHASE_LABEL } from "../lib/compareView";
import { positionsAt, useReplay, type ReplayStore } from "../lib/replay";
import type { Comparison, CornerComparison } from "../lib/types";
import { Badge, Button } from "./ui";

/**
 * Fiche virage : vitesse, frein et gaz des deux tours en fonction de la distance, découpés en phases
 * (freinage, entrée, milieu, sortie : analysis/phases.rs), avec un curseur partagé avec la carte et
 * la relecture (survol ou glisser = la voiture se place à cet endroit sur la carte).
 */
export function CornerSheet({ cmp, corner, store, onClose, onPrev, onNext }: {
  cmp: Comparison;
  corner: CornerComparison;
  store: ReplayStore;
  onClose: () => void;
  onPrev?: () => void;
  onNext?: () => void;
}) {
  const host = useRef<HTMLDivElement>(null);
  const [w, setW] = useState(640);
  useEffect(() => {
    const el = host.current;
    if (!el) return;
    setW(el.clientWidth || 640);
    const ro = new ResizeObserver(() => setW(el.clientWidth || 640));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  const rs = useReplay(store);

  const k = corner;
  const b = k.braking;
  const step = cmp.step_m;
  // Fenêtre : 30 m avant le premier freinage (ou la première phase), jusqu'à la sortie du virage.
  const firstM = Math.min(k.phases[0]?.range_m[0] ?? k.corner.entry_m, b.reference?.start_m ?? Infinity, b.compared?.start_m ?? Infinity, k.corner.entry_m);
  const m0 = Math.max(0, firstM - 30);
  const m1 = Math.min(cmp.length_m, k.corner.exit_m);
  const i0 = Math.max(0, Math.floor(m0 / step));
  const i1 = Math.min(cmp.ref_trace.speed_kmh.length - 1, Math.ceil(m1 / step));

  const PAD_L = 48, PAD_R = 12, TOP = 22;
  const rows = [
    { key: "speed", label: "Vitesse", unit: "km/h", h: 120, get: (t: Comparison["ref_trace"]) => t.speed_kmh },
    { key: "brake", label: "Frein", unit: "%", h: 56, get: (t: Comparison["ref_trace"]) => t.brake.map((v) => v * 100) },
    { key: "gas", label: "Gaz", unit: "%", h: 56, get: (t: Comparison["ref_trace"]) => t.throttle.map((v) => v * 100) },
  ] as const;
  const GAP = 12;
  const H = TOP + rows.reduce((s, r) => s + r.h + GAP, 0);
  const xOf = (m: number) => PAD_L + ((m - m0) / Math.max(1, m1 - m0)) * (w - PAD_L - PAD_R);
  const mOf = (x: number) => m0 + ((x - PAD_L) / Math.max(1, w - PAD_L - PAD_R)) * (m1 - m0);

  let y = TOP;
  const layout = rows.map((r) => {
    const ref = r.get(cmp.ref_trace).slice(i0, i1 + 1);
    const cur = r.get(cmp.cmp_trace).slice(i0, i1 + 1);
    const lo = r.key === "speed" ? Math.floor(Math.min(...ref, ...cur) / 20) * 20 : 0;
    const hi = r.key === "speed" ? Math.ceil(Math.max(...ref, ...cur) / 20) * 20 : 100;
    const top = y;
    y += r.h + GAP;
    const yOf = (v: number) => top + r.h - ((v - lo) / Math.max(1, hi - lo)) * r.h;
    const path = (vals: number[]) => vals.map((v, j) => `${j ? "L" : "M"}${xOf((i0 + j) * step).toFixed(1)} ${yOf(v).toFixed(1)}`).join("");
    return { ...r, top, lo, hi, yOf, refD: path(ref), curD: path(cur), ref, cur };
  });

  // Curseur : la position de la relecture (ta voiture) ; survol ou glisser la déplace.
  const { ci } = positionsAt(cmp, rs.t, rs.ghost);
  const curM = ci * step;
  const inWin = curM >= m0 && curM <= m1;
  const seekAtX = (clientX: number) => {
    const r = host.current?.getBoundingClientRect();
    if (!r) return;
    const m = Math.max(m0, Math.min(m1, mOf(clientX - r.left)));
    const i = Math.round(m / step);
    const t = cmp.cmp_trace.time[i];
    if (t != null && !rs.playing) store.seek(t, true);
  };
  const dragging = useRef(false);
  const j = Math.max(0, Math.min(i1 - i0, Math.round(curM / step) - i0));

  const facts = [brakeGapText(b) && `Freinage : ${brakeGapText(b)}`, minSpeedText(b), throttleGapText(b)].filter(Boolean) as string[];

  return (
    <section className="card corner-sheet" aria-label={`Fiche du virage ${k.corner.name}`}>
      <header className="ui-card-head">
        <h2>
          {k.corner.name} <Badge tone={k.time_delta_s > 0.02 ? "loss" : k.time_delta_s < -0.02 ? "gain" : "neutral"}>{lossText(k.time_delta_s)}</Badge>
        </h2>
        <div className="row" style={{ gap: 8 }}>
          {onPrev && <Button variant="quiet" onClick={onPrev} aria-label="Virage précédent">‹ Précédent</Button>}
          {onNext && <Button variant="quiet" onClick={onNext} aria-label="Virage suivant">Suivant ›</Button>}
          <Button onClick={onClose}>Fermer</Button>
        </div>
      </header>
      <p className="corner-sheet-say">{k.braking_sentence}</p>
      {facts.length > 0 && (
        <div className="corner-sheet-facts">
          {facts.map((f) => <span key={f}>{f}</span>)}
        </div>
      )}
      <div
        ref={host}
        className="corner-sheet-chart"
        onPointerDown={(e) => { dragging.current = true; (e.currentTarget as Element).setPointerCapture?.(e.pointerId); seekAtX(e.clientX); }}
        onPointerMove={(e) => { if (dragging.current || e.pointerType === "mouse") seekAtX(e.clientX); }}
        onPointerUp={() => { dragging.current = false; }}
      >
        <svg width={w} height={H} role="img" aria-label={`Vitesse, frein et gaz dans ${k.corner.name}, référence en pointillé, ton tour en bleu`}>
          {/* Phases : bandes alternées et nom au-dessus. */}
          {k.phases.map((p, n) => {
            const x0 = xOf(Math.max(m0, p.range_m[0])), x1 = xOf(Math.min(m1, p.range_m[1]));
            if (x1 - x0 < 2) return null;
            return (
              <g key={p.phase}>
                <rect x={x0} y={TOP - 4} width={x1 - x0} height={H - TOP} fill={n % 2 ? "var(--surface-2)" : "transparent"} opacity={0.6} />
                {x1 - x0 > 96 && (
                  <text x={(x0 + x1) / 2} y={14} textAnchor="middle" fontSize={12} fill={p.delta_s > 0.02 ? "var(--loss)" : p.delta_s < -0.02 ? "var(--gain)" : "var(--text-2)"}>
                    {PHASE_LABEL[p.phase][0]!.toUpperCase() + PHASE_LABEL[p.phase].slice(1)} {lossText(p.delta_s)}
                  </text>
                )}
              </g>
            );
          })}
          {layout.map((r) => (
            <g key={r.key}>
              <text x={4} y={r.top + 12} fontSize={12} fill="var(--text-2)">{r.label}</text>
              <text x={4} y={r.top + 26} fontSize={12} fill="var(--text-2)">{r.unit}</text>
              <line x1={PAD_L} x2={w - PAD_R} y1={r.top + r.h} y2={r.top + r.h} stroke="var(--line)" />
              {r.key === "speed" && <text x={PAD_L + 4} y={r.top + 12} fontSize={12} fill="var(--text-2)">{r.hi}</text>}
              {r.key === "speed" && <text x={PAD_L + 4} y={r.top + r.h - 4} fontSize={12} fill="var(--text-2)">{r.lo}</text>}
              <path d={r.refD} stroke="var(--ref)" strokeWidth={1.5} strokeDasharray="4 3" fill="none" />
              <path d={r.curD} stroke="var(--info)" strokeWidth={2} fill="none" />
            </g>
          ))}
          {/* Points de freinage mesurés (F) des deux tours. */}
          {b.reference && <line x1={xOf(b.reference.start_m)} x2={xOf(b.reference.start_m)} y1={TOP} y2={H - GAP} stroke="var(--ref)" strokeDasharray="2 3" />}
          {b.compared && <line x1={xOf(b.compared.start_m)} x2={xOf(b.compared.start_m)} y1={TOP} y2={H - GAP} stroke="var(--pedal-brake)" strokeWidth={1.5} />}
          {inWin && (
            <g>
              <line x1={xOf(curM)} x2={xOf(curM)} y1={TOP - 4} y2={H - GAP} stroke="var(--text)" strokeWidth={1} />
              {layout.map((r) => (
                <text key={r.key} x={Math.min(xOf(curM) + 6, w - 120)} y={r.top + 14} fontSize={12} fill="var(--text)" stroke="var(--bg-sunken)" strokeWidth={3} paintOrder="stroke">
                  réf. {Math.round(r.ref[j] ?? 0)} · toi {Math.round(r.cur[j] ?? 0)} {r.unit}
                </text>
              ))}
            </g>
          )}
        </svg>
      </div>
      <p className="muted t-sm m-0">
        Pointillé : référence ; bleu : ton tour ; traits verticaux : points de freinage mesurés au canal frein (pédale à 10 %). Survole ou glisse : la voiture se place à cet endroit sur la carte. Distance de {Math.round(m0)} à {Math.round(m1)} m.
      </p>
    </section>
  );
}
