import { lapTime } from "../lib/format";
import { direction, type SessionPoint } from "../lib/trend";

const N = 12;

function Bars({ values, fmt, higherIsBetter, color }: { values: (number | null)[]; fmt: (v: number) => string; higherIsBetter: boolean; color: string }) {
  const v = values.filter((x): x is number => x != null);
  if (v.length === 0) return <div className="muted trend-empty">pas encore de mesure</div>;
  const lo = Math.min(...v), hi = Math.max(...v);
  // Hauteur = qualité : plus haut = mieux, quel que soit le sens de la mesure.
  const q = (x: number) => (hi === lo ? 0.6 : higherIsBetter ? 0.15 + (0.85 * (x - lo)) / (hi - lo) : 0.15 + (0.85 * (hi - x)) / (hi - lo));
  const W = 260, H = 64, bw = W / N;
  return (
    <svg viewBox={`0 0 ${W} ${H}`} className="trend-bars" role="img">
      {values.map((x, i) => {
        const at = (N - values.length + i) * bw;
        return x == null ? (
          <rect key={i} x={at + 3} y={H - 3} width={bw - 6} height={3} fill="#383848"><title>pas de mesure</title></rect>
        ) : (
          <rect key={i} x={at + 3} y={H - q(x) * H} width={bw - 6} height={q(x) * H} rx={2} fill={color} opacity={i === values.length - 1 ? 1 : 0.55}>
            <title>{fmt(x)}</title>
          </rect>
        );
      })}
    </svg>
  );
}

const DIR_FR = { better: ["en progrès", "neg"], worse: ["en recul", "pos"], flat: ["stable", "muted"] } as const;

/**
 * Tes 3 indicateurs, séance par séance (12 dernières sur ce combo) : rester en piste, régularité,
 * meilleur tour propre. Barre plus haute = mieux. La dernière séance est en couleur pleine.
 */
export function SessionTrend({ points }: { points: SessionPoint[] }) {
  const last = points.slice(-N);
  if (last.length < 2) return null;
  const cards: { title: string; values: (number | null)[]; fmt: (v: number) => string; hib: boolean; color: string; hint: string }[] = [
    { title: "Rester en piste", values: last.map((p) => p.clean_rate), fmt: (v) => `${Math.round(v * 100)} % de tours propres`, hib: true, color: "#3dd9b3", hint: "part de tours propres" },
    { title: "Régularité", values: last.map((p) => p.spread_s), fmt: (v) => `± ${v.toFixed(2).replace(".", ",")} s entre tes tours`, hib: false, color: "#5cc8ff", hint: "écart entre tes tours propres" },
    { title: "Rythme", values: last.map((p) => p.best_s), fmt: (v) => `meilleur tour propre ${lapTime(v)}`, hib: false, color: "#a78bfa", hint: "meilleur tour propre" },
  ];
  return (
    <div className="card" style={{ marginBottom: 12 }}>
      <div className="row" style={{ justifyContent: "space-between" }}>
        <h2 style={{ margin: 0 }}>Séance par séance ({last.length} dernières)</h2>
        <span className="muted" style={{ fontSize: 14 }}>barre plus haute = mieux · survole une barre pour le chiffre</span>
      </div>
      <div className="trend-grid">
        {cards.map((c) => {
          const d = direction(c.values, c.hib);
          const lastV = c.values[c.values.length - 1];
          return (
            <div key={c.title} className="trend">
              <div className="row" style={{ justifyContent: "space-between" }}>
                <b>{c.title}</b>
                {d && <span className={DIR_FR[d][1]}>{DIR_FR[d][0]}</span>}
              </div>
              <Bars values={c.values} fmt={c.fmt} higherIsBetter={c.hib} color={c.color} />
              <div className="muted trend-last">{lastV != null ? `Dernière séance : ${c.fmt(lastV)}` : `Dernière séance : pas de ${c.hint}`}</div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
