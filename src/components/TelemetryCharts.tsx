import { useCallback, useEffect, useMemo, useRef } from "react";
import uPlot from "uplot";
import { decimate } from "../lib/format";
import { positionsAt, type ReplayStore } from "../lib/replay";
import type { Comparison } from "../lib/types";

/** Vrai pendant qu'on place le curseur par programme (relecture) : on ignore alors l'événement. */
let programmatic = false;

const REF = "#c9c9d4";
const CMP = "#ff2a1f";
const GRID = { stroke: "#25252f", width: 1 };
const FONT = "12px 'Archivo Variable', system-ui, sans-serif";
const AXIS = { stroke: "#8c8c9c", grid: GRID, ticks: { stroke: "#2c2c38", size: 4 }, font: FONT };
/** Largeur de la fenêtre glissante des courbes pendant la relecture (m). */
const WINDOW_M = 700;

interface ChartSpec {
  key: string;
  title: string;
  unit: string;
  digits: number;
  height: number;
  series: { label: string; color: string; data: (number | null)[]; fill?: string; width?: number }[];
  range?: [number, number];
  zeroLine?: boolean;
  /** Affiche le signe (+/−) dans la valeur au curseur. */
  signed?: boolean;
}

/** Un graphique uPlot ; le curseur est partagé via `syncKey`. */
function SyncedChart({ x, spec, syncKey, onCursor, markers, onReady, showX }: {
  x: number[];
  spec: ChartSpec;
  syncKey: string;
  onCursor: (idx: number | null) => void;
  markers: { at: number; label: string }[];
  onReady: (key: string, u: uPlot | null) => void;
  showX: boolean;
}) {
  const ready = useRef(onReady);
  ready.current = onReady;
  const host = useRef<HTMLDivElement>(null);
  const readout = useRef<HTMLSpanElement>(null);
  const plot = useRef<uPlot | null>(null);
  const cursorCb = useRef(onCursor);
  cursorCb.current = onCursor;

  useEffect(() => {
    const el = host.current;
    if (!el) return;
    const fmt = (v: number | null | undefined) => {
      if (v == null) return "—";
      const r = Math.abs(v) < 0.5 * 10 ** -spec.digits ? 0 : v;
      return (spec.signed && r > 0 ? "+" : "") + r.toFixed(spec.digits).replace("-", "−");
    };
    const opts: uPlot.Options = {
      width: el.clientWidth,
      height: spec.height,
      cursor: { sync: { key: syncKey, setSeries: false }, drag: { x: true, y: false }, points: { size: 6 } },
      legend: { show: false },
      scales: { x: { time: false }, y: spec.range ? { range: spec.range } : { auto: true } },
      axes: [
        showX
          ? { ...AXIS, size: 26, space: 70, values: (_u, v) => v.map((m) => `${(m / 1000).toFixed(1)} km`) }
          : { ...AXIS, size: 4, space: 70, values: (_u, v) => v.map(() => "") },
        { ...AXIS, size: 44, space: 28 },
      ],
      series: [{}, ...spec.series.map((s) => ({ label: s.label, stroke: s.color, width: s.width ?? 1.6, fill: s.fill, points: { show: false } }))],
      hooks: {
        setCursor: [
          (u) => {
            const i = u.cursor.idx;
            // Valeurs au curseur dans le titre (lecture directe, sans survoler la légende).
            if (readout.current) {
              readout.current.textContent = i == null ? "" : spec.series.map((s, k) => `${spec.series.length > 1 ? `${s.label} ` : ""}${fmt(u.data[k + 1]?.[i] as number | null)}`).join("  ·  ") + (i == null ? "" : ` ${spec.unit}`);
            }
            if (!programmatic) cursorCb.current(i ?? null);
          },
        ],
        draw: [
          (u) => {
            const ctx = u.ctx;
            ctx.save();
            if (spec.zeroLine) {
              const y = u.valToPos(0, "y", true);
              ctx.strokeStyle = "rgba(255,255,255,0.35)";
              ctx.beginPath();
              ctx.moveTo(u.bbox.left, y);
              ctx.lineTo(u.bbox.left + u.bbox.width, y);
              ctx.stroke();
            }
            ctx.fillStyle = "#9a9aab";
            ctx.strokeStyle = "rgba(154,154,171,0.18)";
            ctx.font = `italic 800 ${11 * devicePixelRatio}px "Archivo Variable", system-ui, sans-serif`;
            // Noms décalés sur plusieurs lignes quand les virages sont proches.
            const rowEnd: number[] = [];
            for (const m of markers) {
              const px = u.valToPos(m.at, "x", true);
              if (px < u.bbox.left - 2 || px > u.bbox.left + u.bbox.width) continue;
              ctx.beginPath();
              ctx.moveTo(px, u.bbox.top);
              ctx.lineTo(px, u.bbox.top + u.bbox.height);
              ctx.stroke();
              const w = ctx.measureText(m.label).width;
              let row = rowEnd.findIndex((end) => end < px);
              if (row < 0) row = rowEnd.length;
              rowEnd[row] = px + 3 + w + 6 * devicePixelRatio;
              ctx.fillText(m.label, px + 3, u.bbox.top + (11 + row * 12) * devicePixelRatio);
            }
            ctx.restore();
          },
        ],
      },
    };
    const data = [x, ...spec.series.map((s) => s.data)] as uPlot.AlignedData;
    plot.current = new uPlot(opts, data, el);
    ready.current(spec.key, plot.current);
    // Écran tactile : glisser le doigt horizontalement déplace le curseur (le défilement vertical reste possible).
    const u0 = plot.current;
    const onTouch = (e: PointerEvent) => {
      if (e.pointerType !== "touch") return;
      const r = u0.over.getBoundingClientRect();
      const left = Math.max(0, Math.min(r.width, e.clientX - r.left));
      u0.setCursor({ left, top: r.height / 2 });
    };
    u0.over.style.touchAction = "pan-y";
    u0.over.addEventListener("pointerdown", onTouch);
    u0.over.addEventListener("pointermove", onTouch);
    const ro = new ResizeObserver(() => plot.current?.setSize({ width: el.clientWidth, height: spec.height }));
    ro.observe(el);
    return () => {
      ro.disconnect();
      ready.current(spec.key, null);
      plot.current?.destroy();
      plot.current = null;
    };
  }, [x, spec, syncKey, markers, showX]);

  return (
    <div className="chart">
      <div className="chart-title">
        <span>{spec.title}</span>
        <span ref={readout} className="chart-readout" />
      </div>
      <div ref={host} />
    </div>
  );
}

/** Pile de courbes : delta, vitesse, frein, accélérateur, trajectoire, rapport, volant. */
export function TelemetryCharts({ cmp, onCursor, simple = false, store }: { cmp: Comparison; onCursor: (idx: number | null) => void; simple?: boolean; store?: ReplayStore }) {
  const plots = useRef(new Map<string, uPlot>());
  const onReady = useCallback((key: string, u: uPlot | null) => {
    if (u) plots.current.set(key, u);
    else plots.current.delete(key);
  }, []);
  // Affichage à ≥2 m de résolution : fluide même sur un tour de 13 km.
  const every = Math.max(1, Math.round(2 / cmp.step_m));
  const { x, specs, markers } = useMemo(() => {
    const d = (a: number[] | null) => (a ? decimate(a, every) : null);
    const r = cmp.ref_trace;
    const c = cmp.cmp_trace;
    const pct = (a: number[]) => d(a)!.map((v) => v * 100);
    const specs: ChartSpec[] = [
      { key: "delta", title: "Écart de temps", unit: "s", digits: 3, height: 110, zeroLine: true, signed: true, series: [{ label: "Écart", color: "#ffffff", data: d(cmp.delta_s)!, fill: "rgba(255,212,0,0.10)" }] },
      { key: "speed", title: "Vitesse", unit: "km/h", digits: 0, height: 170, series: [{ label: "Réf.", color: REF, data: d(r.speed_kmh)! }, { label: "Tour", color: CMP, data: d(c.speed_kmh)! }] },
      { key: "brake", title: "Frein", unit: "%", digits: 0, height: 80, range: [0, 100], series: [{ label: "Réf.", color: REF, data: pct(r.brake) }, { label: "Tour", color: CMP, data: pct(c.brake) }] },
      { key: "throttle", title: "Accélérateur", unit: "%", digits: 0, height: 80, range: [0, 100], series: [{ label: "Réf.", color: REF, data: pct(r.throttle) }, { label: "Tour", color: CMP, data: pct(c.throttle) }] },
    ];
    if (cmp.line_offset_m) {
      specs.push({ key: "line", title: "Trajectoire : écart à la référence (au-dessus de zéro = plus à gauche)", unit: "m", digits: 1, height: 90, zeroLine: true, series: [{ label: "Écart", color: CMP, data: d(cmp.line_offset_m)!, fill: "rgba(255,42,31,0.08)" }] });
    }
    if (!simple && r.gear && c.gear) {
      specs.push({ key: "gear", title: "Rapport", unit: "", digits: 0, height: 64, series: [{ label: "Réf.", color: REF, data: d(r.gear)! }, { label: "Tour", color: CMP, data: d(c.gear)! }] });
    }
    if (!simple && r.steer_deg && c.steer_deg) {
      specs.push({ key: "steer", title: "Volant", unit: "°", digits: 0, height: 90, zeroLine: true, series: [{ label: "Réf.", color: REF, data: d(r.steer_deg)! }, { label: "Tour", color: CMP, data: d(c.steer_deg)! }] });
    }
    const markers = cmp.corners.map((k) => ({ at: k.corner.apex_m, label: k.corner.name }));
    return { x: decimate(r.dist, every), specs, markers };
  }, [cmp, every, simple]);

  // Relecture : curseur qui suit la voiture et, si activé, fenêtre glissante autour d'elle.
  useEffect(() => {
    if (!store) return;
    let lastWin: number | null = null;
    let wasFollow = store.get().chartFollow;
    const xmax = x[x.length - 1] ?? 0;
    const apply = () => {
      const st = store.get();
      const { ci } = positionsAt(cmp, st.t);
      const xv = x[Math.min(x.length - 1, Math.round(ci / every))];
      if (xv == null) return;
      programmatic = true;
      try {
        if (st.chartFollow && !st.fromChart && (lastWin == null || Math.abs(xv - lastWin) > 1.5)) {
          lastWin = xv;
          const min = Math.max(0, Math.min(xmax - WINDOW_M, xv - WINDOW_M * 0.35));
          for (const u of plots.current.values()) u.setScale("x", { min, max: min + WINDOW_M });
        } else if (!st.chartFollow && wasFollow) {
          lastWin = null;
          for (const u of plots.current.values()) u.setScale("x", { min: 0, max: xmax });
        }
        wasFollow = st.chartFollow;
        for (const u of plots.current.values()) {
          u.setCursor({ left: u.valToPos(xv, "x"), top: u.over.clientHeight / 2 });
        }
      } finally {
        programmatic = false;
      }
    };
    return store.subscribe(apply);
  }, [store, cmp, x, every]);

  const missing = [!cmp.ref_trace.gear || !cmp.cmp_trace.gear ? "rapport" : null, !cmp.ref_trace.steer_deg || !cmp.cmp_trace.steer_deg ? "volant" : null].filter(Boolean);

  return (
    <div className="charts">
      <div className="row" style={{ justifyContent: "space-between", marginBottom: 6 }}>
        <div className="legend">
          <span><i style={{ background: REF }} />Référence (tour {cmp.reference.number})</span>
          <span><i style={{ background: CMP }} />Tour analysé ({cmp.compared.number})</span>
        </div>
        {missing.length > 0 && !simple && <span className="muted" style={{ fontSize: 12 }}>Canaux non disponibles : {missing.join(", ")}</span>}
      </div>
      {specs.map((s, i) => (
        <SyncedChart
          key={s.key}
          x={x}
          spec={s}
          syncKey="apex"
          markers={markers}
          onReady={onReady}
          showX={i === specs.length - 1}
          onCursor={(k) => onCursor(k == null ? null : Math.min(k * every, cmp.ref_trace.dist.length - 1))}
        />
      ))}
    </div>
  );
}
