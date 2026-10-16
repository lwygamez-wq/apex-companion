import { useEffect, useMemo, useRef } from "react";
import uPlot from "uplot";
import { decimate } from "../lib/format";
import type { Comparison } from "../lib/types";

const REF = "#c3c9cf";
const CMP = "#f4c20d";
const GRID = { stroke: "#33383e", width: 1 };
const FONT = "12px 'Archivo Variable', system-ui, sans-serif";
const AXIS = { stroke: "#9aa1a9", grid: GRID, ticks: { stroke: "#3a4047" }, font: FONT };

interface ChartSpec {
  key: string;
  title: string;
  height: number;
  series: { label: string; color: string; data: (number | null)[]; fill?: string }[];
  range?: [number, number];
}

/** Un graphique uPlot synchronisé (curseur partagé via `syncKey`). */
function SyncedChart({ x, spec, syncKey, onCursor, markers }: {
  x: number[];
  spec: ChartSpec;
  syncKey: string;
  onCursor: (idx: number | null) => void;
  markers: { at: number; label: string }[];
}) {
  const host = useRef<HTMLDivElement>(null);
  const plot = useRef<uPlot | null>(null);
  const cursorCb = useRef(onCursor);
  cursorCb.current = onCursor;

  useEffect(() => {
    const el = host.current;
    if (!el) return;
    const opts: uPlot.Options = {
      width: el.clientWidth,
      height: spec.height,
      cursor: { sync: { key: syncKey, setSeries: false }, drag: { x: true, y: false }, points: { size: 5 } },
      legend: { show: false },
      scales: { x: { time: false }, y: spec.range ? { range: spec.range } : { auto: true } },
      axes: [
        { ...AXIS, size: 30, space: 70, values: (_u, v) => v.map((m) => `${(m / 1000).toFixed(1)} km`) },
        { ...AXIS, size: 46 },
      ],
      series: [{}, ...spec.series.map((s) => ({ label: s.label, stroke: s.color, width: 1.4, fill: s.fill, points: { show: false } }))],
      hooks: {
        setCursor: [(u) => cursorCb.current(u.cursor.idx ?? null)],
        draw: [
          (u) => {
            // Repères de virages.
            const ctx = u.ctx;
            ctx.save();
            ctx.fillStyle = "#9aa1a9";
            ctx.strokeStyle = "rgba(154,161,169,0.22)";
            ctx.font = `600 ${11 * devicePixelRatio}px "Archivo Variable", system-ui, sans-serif`;
            for (const m of markers) {
              const px = u.valToPos(m.at, "x", true);
              ctx.beginPath();
              ctx.moveTo(px, u.bbox.top);
              ctx.lineTo(px, u.bbox.top + u.bbox.height);
              ctx.stroke();
              ctx.fillText(m.label, px + 3, u.bbox.top + 11 * devicePixelRatio);
            }
            ctx.restore();
          },
        ],
      },
    };
    const data = [x, ...spec.series.map((s) => s.data)] as uPlot.AlignedData;
    plot.current = new uPlot(opts, data, el);
    const ro = new ResizeObserver(() => plot.current?.setSize({ width: el.clientWidth, height: spec.height }));
    ro.observe(el);
    return () => {
      ro.disconnect();
      plot.current?.destroy();
      plot.current = null;
    };
  }, [x, spec, syncKey, markers]);

  return (
    <div className="chart">
      <div className="chart-title">{spec.title}</div>
      <div ref={host} />
    </div>
  );
}

/** Pile de courbes : delta, vitesse, frein, accélérateur, rapport, volant. */
export function TelemetryCharts({ cmp, onCursor, simple = false }: { cmp: Comparison; onCursor: (idx: number | null) => void; simple?: boolean }) {
  // Affichage à ≥2 m de résolution : fluide même sur un tour de 13 km.
  const every = Math.max(1, Math.round(2 / cmp.step_m));
  const { x, specs, markers } = useMemo(() => {
    const d = (a: number[] | null) => (a ? decimate(a, every) : null);
    const r = cmp.ref_trace;
    const c = cmp.cmp_trace;
    const specs: ChartSpec[] = [
      { key: "delta", title: "Écart de temps (s), au-dessus de zéro : temps perdu", height: 120, series: [{ label: "Delta", color: "#e6e8ea", data: d(cmp.delta_s)!, fill: "rgba(244,194,13,0.12)" }] },
      { key: "speed", title: "Vitesse (km/h)", height: 170, series: [{ label: "Réf.", color: REF, data: d(r.speed_kmh)! }, { label: "Tour", color: CMP, data: d(c.speed_kmh)! }] },
      { key: "brake", title: "Frein (%)", height: 90, range: [0, 100], series: [{ label: "Réf.", color: REF, data: d(r.brake)!.map((v) => v * 100) }, { label: "Tour", color: CMP, data: d(c.brake)!.map((v) => v * 100) }] },
      { key: "throttle", title: "Accélérateur (%)", height: 90, range: [0, 100], series: [{ label: "Réf.", color: REF, data: d(r.throttle)!.map((v) => v * 100) }, { label: "Tour", color: CMP, data: d(c.throttle)!.map((v) => v * 100) }] },
    ];
    if (!simple && r.gear && c.gear) {
      specs.push({ key: "gear", title: "Rapport", height: 70, series: [{ label: "Réf.", color: REF, data: d(r.gear)! }, { label: "Tour", color: CMP, data: d(c.gear)! }] });
    }
    if (!simple && r.steer_deg && c.steer_deg) {
      specs.push({ key: "steer", title: "Volant (°)", height: 90, series: [{ label: "Réf.", color: REF, data: d(r.steer_deg)! }, { label: "Tour", color: CMP, data: d(c.steer_deg)! }] });
    }
    const markers = cmp.corners.map((k) => ({ at: k.corner.apex_m, label: k.corner.name }));
    return { x: decimate(r.dist, every), specs, markers };
  }, [cmp, every, simple]);

  const missing = [!cmp.ref_trace.gear || !cmp.cmp_trace.gear ? "rapport" : null, !cmp.ref_trace.steer_deg || !cmp.cmp_trace.steer_deg ? "volant" : null].filter(Boolean);

  return (
    <div>
      <div className="row" style={{ justifyContent: "space-between", marginBottom: 6 }}>
        <div className="legend">
          <span><i style={{ background: REF }} />Référence (tour {cmp.reference.number})</span>
          <span><i style={{ background: CMP }} />Tour analysé ({cmp.compared.number})</span>
        </div>
        {missing.length > 0 && <span className="muted" style={{ fontSize: 12 }}>Canaux non disponibles : {missing.join(", ")}</span>}
      </div>
      {specs.map((s) => (
        <SyncedChart key={s.key} x={x} spec={s} syncKey="apex" markers={markers} onCursor={(i) => onCursor(i == null ? null : Math.min(i * every, cmp.ref_trace.dist.length - 1))} />
      ))}
    </div>
  );
}
