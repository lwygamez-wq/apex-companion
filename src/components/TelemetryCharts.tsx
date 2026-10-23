import { useCallback, useEffect, useMemo, useRef } from "react";
import uPlot from "uplot";
import { buildView, type Axis, type CompareView, type GapUnit } from "../lib/compareView";
import { at, positionsAt, type ReplayStore } from "../lib/replay";
import type { Comparison } from "../lib/types";

/** Vrai pendant qu'on place le curseur par programme (relecture) : on ignore alors l'événement. */
let programmatic = false;

export const REF_COLOR = "#8fb4ff";
export const CMP_COLOR = "#ff3b30";
const THR = "#2bd576";
const BRK = "#ff5a4e";
const LOSS = "#ffd400";
const GRID = { stroke: "#22222c", width: 1 };
const FONT = "12px 'Archivo Variable', system-ui, sans-serif";
const AXIS = { stroke: "#8c8c9c", grid: GRID, ticks: { show: false }, font: FONT };
/** Largeur de la fenêtre glissante pendant la relecture. */
const WINDOW = { distance: 700, time: 12 };

interface Series {
  label: string;
  color: string;
  data: (number | null)[];
  width?: number;
  dash?: number[];
  fill?: string;
  /** Valeur affichée sans signe (frein tracé vers le bas). */
  abs?: boolean;
  /** Pas de valeur au curseur (séries de remplissage). */
  silent?: boolean;
}

interface ChartSpec {
  key: string;
  title: string;
  unit: string;
  digits: number;
  height: number;
  series: Series[];
  range?: [number, number];
  zeroLine?: boolean;
  signed?: boolean;
  /** Noms des virages affichés en haut de ce graphique. */
  labels?: boolean;
  /** Légende sous le titre (pédales). */
  hint?: string;
}

const fmtTime = (t: number) => {
  const m = Math.floor(t / 60);
  const s = t - m * 60;
  return m > 0 ? `${m}:${s.toFixed(0).padStart(2, "0")}` : `${s.toFixed(0)} s`;
};

/** Un graphique uPlot ; le curseur est partagé via `syncKey`. */
function SyncedChart({ view, spec, onCursor, selected, onReady, showX, syncKey }: {
  syncKey: string;
  view: CompareView;
  spec: ChartSpec;
  onCursor: (idx: number | null) => void;
  selected: number | null;
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
  const sel = useRef(selected);
  sel.current = selected;

  useEffect(() => {
    const el = host.current;
    if (!el) return;
    const fmt = (v: number | null | undefined, s: Series) => {
      if (v == null) return s.abs ? "0" : "—";
      const x = s.abs ? Math.abs(v) : v;
      const r = Math.abs(x) < 0.5 * 10 ** -spec.digits ? 0 : x;
      return (spec.signed && r > 0 ? "+" : "") + r.toFixed(spec.digits).replace("-", "−");
    };
    const xFmt = view.axis === "distance" ? (m: number) => `${(m / 1000).toFixed(1)} km` : fmtTime;
    const opts: uPlot.Options = {
      width: el.clientWidth,
      height: spec.height,
      padding: [spec.labels ? 18 : 6, 8, 0, 0],
      cursor: { sync: { key: syncKey, setSeries: false }, drag: { x: true, y: false }, points: { size: 5 } },
      legend: { show: false },
      scales: { x: { time: false }, y: spec.range ? { range: spec.range } : { auto: true } },
      axes: [
        showX ? { ...AXIS, size: 24, space: 80, values: (_u, v) => v.map(xFmt) } : { ...AXIS, size: 0, values: () => [] },
        { ...AXIS, size: 40, space: 30, values: (_u, v) => v.map((n) => (spec.key === "pedals" ? `${Math.abs(n)}` : `${n}`)) },
      ],
      series: [
        {},
        ...spec.series.map((s) => ({ label: s.label, stroke: s.color, width: s.width ?? 1.5, dash: s.dash, fill: s.fill, fillTo: 0, points: { show: false } })),
      ],
      hooks: {
        setCursor: [
          (u) => {
            const i = u.cursor.idx;
            if (readout.current) {
              const shown = spec.series.filter((s) => !s.silent);
              readout.current.textContent =
                i == null ? "" : shown.map((s) => `${shown.length > 1 ? `${s.label} ` : ""}${fmt(u.data[spec.series.indexOf(s) + 1]?.[i] as number | null, s)}`).join("   ") + (spec.unit ? ` ${spec.unit}` : "");
            }
            // Seul un vrai survol (souris, doigt) déplace la lecture : pas les mises à jour internes de uPlot.
            if (!programmatic && hovering) cursorCb.current(i ?? null);
          },
        ],
        drawClear: [
          (u) => {
            // Zones de virage, sous les courbes.
            const ctx = u.ctx;
            ctx.save();
            for (const b of view.bands) {
              const a = u.valToPos(b.from, "x", true);
              const z = u.valToPos(b.to, "x", true);
              if (z < u.bbox.left || a > u.bbox.left + u.bbox.width) continue;
              ctx.fillStyle = sel.current === b.id ? "rgba(255,255,255,0.10)" : "rgba(255,255,255,0.035)";
              ctx.fillRect(Math.max(a, u.bbox.left), u.bbox.top, Math.min(z, u.bbox.left + u.bbox.width) - Math.max(a, u.bbox.left), u.bbox.height);
            }
            ctx.restore();
          },
        ],
        draw: [
          (u) => {
            const ctx = u.ctx;
            ctx.save();
            if (spec.zeroLine) {
              const y = u.valToPos(0, "y", true);
              ctx.strokeStyle = "rgba(255,255,255,0.25)";
              ctx.beginPath();
              ctx.moveTo(u.bbox.left, y);
              ctx.lineTo(u.bbox.left + u.bbox.width, y);
              ctx.stroke();
            }
            if (spec.labels) {
              ctx.font = `700 ${11 * devicePixelRatio}px "Archivo Variable", system-ui, sans-serif`;
              ctx.textAlign = "center";
              let lastEnd = -Infinity;
              for (const b of view.bands) {
                const px = u.valToPos(b.apex, "x", true);
                if (px < u.bbox.left || px > u.bbox.left + u.bbox.width) continue;
                const w = ctx.measureText(b.label).width;
                // Pas de chevauchement : un nom trop proche du précédent est omis (sauf le virage choisi).
                if (px - w / 2 < lastEnd + 4 * devicePixelRatio && sel.current !== b.id) continue;
                ctx.fillStyle = sel.current === b.id ? "#ffffff" : "#8c8c9c";
                ctx.fillText(b.label, px, u.bbox.top - 5 * devicePixelRatio);
                lastEnd = px + w / 2;
              }
            }
            ctx.restore();
          },
        ],
      },
    };
    let hovering = false;
    const data = [view.x, ...spec.series.map((s) => s.data)] as uPlot.AlignedData;
    plot.current = new uPlot(opts, data, el);
    ready.current(spec.key, plot.current);
    const u0 = plot.current;
    // Écran tactile : glisser le doigt horizontalement déplace le curseur (le défilement vertical reste possible).
    const onTouch = (e: PointerEvent) => {
      if (e.pointerType !== "touch") return;
      const r = u0.over.getBoundingClientRect();
      u0.setCursor({ left: Math.max(0, Math.min(r.width, e.clientX - r.left)), top: r.height / 2 });
    };
    u0.over.addEventListener("pointerenter", () => (hovering = true));
    u0.over.addEventListener("pointerleave", () => (hovering = false));
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
  }, [view, spec, showX, syncKey]);

  // Virage choisi : simple redessin, sans recréer le graphique.
  useEffect(() => plot.current?.redraw(false), [selected]);

  return (
    <div className="chart">
      <div className="chart-title">
        <span>{spec.title}{spec.hint && <span className="chart-hint">{spec.hint}</span>}</span>
        <span ref={readout} className="chart-readout" />
      </div>
      <div ref={host} />
    </div>
  );
}

/**
 * Courbes de comparaison : écart, vitesse, pédales (gaz vers le haut, frein vers le bas) ;
 * trajectoire, rapport et volant en option. Référence en bleu pointillé, ton tour en trait plein.
 */
export function TelemetryCharts({ cmp, onCursorTime, axis = "distance", gapUnit = "s", more = false, selected = null, store, variant = "analysis", fitHeight }: {
  cmp: Comparison;
  onCursorTime: (t: number | null) => void;
  axis?: Axis;
  gapUnit?: GapUnit;
  more?: boolean;
  selected?: number | null;
  store?: ReplayStore;
  /** « studio » : une piste par canal (écart, gaz, frein, vitesse, volant, rapport), à la hauteur `fitHeight`. */
  variant?: "analysis" | "studio";
  fitHeight?: number;
}) {
  const plots = useRef(new Map<string, uPlot>());
  const onReady = useCallback((key: string, u: uPlot | null) => {
    if (u) plots.current.set(key, u);
    else plots.current.delete(key);
  }, []);
  // Mode distance : ≥2 m de résolution, fluide même sur un tour de 13 km.
  const every = Math.max(1, Math.round(2 / cmp.step_m));
  const view = useMemo(() => buildView(cmp, axis, gapUnit, every), [cmp, axis, gapUnit, every]);
  const specs = useMemo(() => {
    // Frein vers le bas ; rien de tracé quand la pédale est relâchée (pas de trait permanent sur le zéro).
    const neg = (a: (number | null)[]) => a.map((v, i) => (v == null || (v < 0.5 && (a[i - 1] ?? 0) < 0.5 && (a[i + 1] ?? 0) < 0.5) ? null : -v));
    const pos = (a: (number | null)[]) => a.map((v) => (v == null ? null : Math.max(0, v)));
    const below = (a: (number | null)[]) => a.map((v) => (v == null ? null : Math.min(0, v)));
    if (variant === "studio") {
      const REFD = "rgba(255,255,255,0.75)";
      const dash = [2, 3];
      const st: ChartSpec[] = [
        { key: "gap", title: gapUnit === "s" ? "Écart (s)" : "Écart (m)", unit: gapUnit, digits: gapUnit === "s" ? 2 : 0, height: 1.2, zeroLine: true, signed: true, labels: true, series: [
          { label: "", color: "transparent", data: pos(view.gap), fill: "rgba(255,212,0,0.16)", width: 0, silent: true },
          { label: "", color: "transparent", data: below(view.gap), fill: "rgba(43,213,118,0.18)", width: 0, silent: true },
          { label: "Écart", color: LOSS, data: view.gap, width: 1.6 },
        ] },
        { key: "thr", title: "Gaz", unit: "%", digits: 0, height: 1, range: [0, 105], series: [{ label: "Réf.", color: REFD, data: view.ref.throttle, dash, width: 1.2 }, { label: "Toi", color: THR, data: view.cmp.throttle, width: 1.6, fill: "rgba(43,213,118,0.16)" }] },
        { key: "brk", title: "Frein", unit: "%", digits: 0, height: 1, range: [0, 105], series: [{ label: "Réf.", color: REFD, data: view.ref.brake, dash, width: 1.2 }, { label: "Toi", color: BRK, data: view.cmp.brake, width: 1.6, fill: "rgba(255,90,78,0.16)" }] },
        { key: "spd", title: "Vitesse", unit: "km/h", digits: 0, height: 1.3, series: [{ label: "Réf.", color: REFD, data: view.ref.speed, dash, width: 1.2 }, { label: "Toi", color: "#46c8e8", data: view.cmp.speed, width: 1.6, fill: "rgba(70,200,232,0.12)" }] },
      ];
      if (view.ref.steer && view.cmp.steer) st.push({ key: "str", title: "Volant", unit: "°", digits: 0, height: 0.9, zeroLine: true, series: [{ label: "Réf.", color: REFD, data: view.ref.steer, dash, width: 1.1 }, { label: "Toi", color: "#c9c9d4", data: view.cmp.steer, width: 1.4 }] });
      if (view.ref.gear && view.cmp.gear) st.push({ key: "gear", title: "Rapport", unit: "", digits: 0, height: 0.8, series: [{ label: "Réf.", color: REFD, data: view.ref.gear, dash, width: 1.1 }, { label: "Toi", color: "#5aa0ff", data: view.cmp.gear, width: 1.6, fill: "rgba(90,160,255,0.12)" }] });
      // Hauteurs relatives → pixels : chaque piste a un titre (≈ 22 px), la dernière l'axe (24 px).
      const total = st.reduce((a, x) => a + x.height, 0);
      const avail = Math.max(st.length * 50, (fitHeight ?? 640) - st.length * 24 - 24);
      for (const x of st) x.height = Math.max(40, Math.floor((x.height / total) * avail));
      return st;
    }
    const s: ChartSpec[] = [
      {
        key: "gap",
        title: gapUnit === "s" ? "Écart (secondes)" : "Écart (mètres)",
        hint: "au-dessus : tu perds · en dessous : tu gagnes",
        unit: gapUnit,
        digits: gapUnit === "s" ? 2 : 0,
        height: 120,
        zeroLine: true,
        signed: true,
        labels: true,
        series: [
          { label: "", color: "transparent", data: pos(view.gap), fill: "rgba(255,212,0,0.16)", width: 0, silent: true },
          { label: "", color: "transparent", data: below(view.gap), fill: "rgba(43,213,118,0.18)", width: 0, silent: true },
          { label: "Écart", color: LOSS, data: view.gap, width: 1.8 },
        ],
      },
      {
        key: "speed",
        title: "Vitesse",
        unit: "km/h",
        digits: 0,
        height: 160,
        series: [
          { label: "Réf.", color: REF_COLOR, data: view.ref.speed, dash: [5, 4], width: 1.4 },
          { label: "Toi", color: CMP_COLOR, data: view.cmp.speed, width: 2 },
        ],
      },
      {
        key: "pedals",
        title: "Pédales",
        hint: "gaz ↑ · frein ↓",
        unit: "%",
        digits: 0,
        height: 130,
        range: [-100, 100],
        zeroLine: true,
        series: [
          { label: "Gaz réf.", color: "rgba(143,180,255,0.9)", data: view.ref.throttle, dash: [5, 4], width: 1.2 },
          { label: "Frein réf.", color: "rgba(143,180,255,0.9)", data: neg(view.ref.brake), dash: [5, 4], width: 1.2, abs: true },
          { label: "Gaz", color: THR, data: view.cmp.throttle, width: 1.8 },
          { label: "Frein", color: BRK, data: neg(view.cmp.brake), width: 1.8, abs: true, fill: "rgba(255,90,78,0.12)" },
        ],
      },
    ];
    if (more && view.line) {
      s.push({ key: "line", title: "Trajectoire", hint: "écart latéral, au-dessus = plus à gauche que la réf.", unit: "m", digits: 1, height: 90, zeroLine: true, series: [{ label: "Écart", color: CMP_COLOR, data: view.line, fill: "rgba(255,59,48,0.08)" }] });
    }
    if (more && view.ref.gear && view.cmp.gear) {
      s.push({ key: "gear", title: "Rapport", unit: "", digits: 0, height: 64, series: [{ label: "Réf.", color: REF_COLOR, data: view.ref.gear, dash: [5, 4] }, { label: "Toi", color: CMP_COLOR, data: view.cmp.gear, width: 1.8 }] });
    }
    if (more && view.ref.steer && view.cmp.steer) {
      s.push({ key: "steer", title: "Volant", unit: "°", digits: 0, height: 90, zeroLine: true, series: [{ label: "Réf.", color: REF_COLOR, data: view.ref.steer, dash: [5, 4] }, { label: "Toi", color: CMP_COLOR, data: view.cmp.steer, width: 1.8 }] });
    }
    return s;
  }, [view, gapUnit, more, variant, fitHeight]);

  // Relecture : curseur qui suit la voiture et, si activé, fenêtre glissante autour d'elle.
  useEffect(() => {
    if (!store) return;
    let lastWin: number | null = null;
    let wasFollow = store.get().chartFollow;
    const xmax = view.x[view.x.length - 1] ?? 0;
    const xmin = view.x[0] ?? 0;
    const win = WINDOW[view.axis];
    const apply = () => {
      const st = store.get();
      const xv = view.axis === "time" ? st.t : at(cmp.cmp_trace.dist, positionsAt(cmp, st.t).ci);
      if (xv == null) return;
      programmatic = true;
      try {
        if (st.chartFollow && !st.fromChart && (lastWin == null || Math.abs(xv - lastWin) > win / 400)) {
          lastWin = xv;
          const min = Math.max(xmin, Math.min(xmax - win, xv - win * 0.35));
          for (const u of plots.current.values()) u.setScale("x", { min, max: min + win });
        } else if (!st.chartFollow && wasFollow) {
          lastWin = null;
          for (const u of plots.current.values()) u.setScale("x", { min: xmin, max: xmax });
        }
        wasFollow = st.chartFollow;
        for (const u of plots.current.values()) u.setCursor({ left: u.valToPos(xv, "x"), top: u.over.clientHeight / 2 });
      } finally {
        programmatic = false;
      }
    };
    return store.subscribe(apply);
  }, [store, cmp, view]);

  return (
    <div className={`charts charts-${variant}`}>
      {specs.map((s, i) => (
        <SyncedChart
          key={`${s.key}-${view.axis}-${view.gapUnit}`}
          view={view}
          spec={s}
          syncKey={`apex-${variant}`}
          selected={selected}
          onReady={onReady}
          showX={i === specs.length - 1}
          onCursor={(k) => onCursorTime(k == null ? null : view.cmpTime[k] ?? null)}
        />
      ))}
    </div>
  );
}
