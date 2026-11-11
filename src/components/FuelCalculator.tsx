import { useEffect, useState } from "react";
import { api } from "../lib/api";
import { lapTime } from "../lib/format";
import { useAsync } from "../lib/hooks";
import type { CircuitOverview, FuelUnit, RaceLength, RaceStrategyView } from "../lib/types";

type Combo = Pick<CircuitOverview, "sim" | "track" | "track_config" | "car" | "setup">;

const n1 = (v: number) => v.toFixed(1).replace(".", ",");
const n2 = (v: number) => v.toFixed(2).replace(".", ",");
/** « 46,0 L » / « 87,4 % ». */
export const amount = (u: FuelUnit, v: number) => (u === "energy" ? `${n1(v)} %` : `${n1(v)} L`);
const duration = (s: number) => {
  const m = Math.round(s / 60);
  return m >= 60 ? `${Math.floor(m / 60)} h ${String(m % 60).padStart(2, "0")}` : `${m} min`;
};
/** « 3:31.4 », « 3:31,4 » ou « 211.4 » → secondes. */
export function parseLapTime(t: string): number | null {
  const s = t.trim().replace(",", ".");
  const m = /^(\d+):(\d{1,2}(?:\.\d+)?)$/.exec(s);
  const v = m ? Number(m[1]) * 60 + Number(m[2]) : Number(s);
  return Number.isFinite(v) && v > 1 ? v : null;
}
const WHEELS = ["AvG", "AvD", "ArG", "ArD"];

/**
 * Calculateur de relais : combien d'arrêts, quand, et avec quel niveau repartir. En % d'énergie
 * virtuelle pour les voitures qui en ont (LMU : Hypercar, LMGT3), sinon en litres. Les valeurs
 * mesurées sur tes tours propres sont pré-remplies et peuvent être remplacées à la main.
 */
export function FuelCalculator({ combo, title = "Calculateur carburant / énergie" }: { combo: Combo; title?: string }) {
  const [length, setLength] = useState<RaceLength | null>(null);
  const [margin, setMargin] = useState(1);
  const [formation, setFormation] = useState(true);
  const [unit, setUnit] = useState<FuelUnit | null>(null);
  const [perLap, setPerLap] = useState("");
  const [lapTxt, setLapTxt] = useState("");
  const [tank, setTank] = useState("");
  const key = `${combo.sim}|${combo.track}|${combo.track_config}|${combo.car}|${combo.setup}`;
  // Autre circuit ou voiture : on repart des mesures.
  useEffect(() => {
    setLength(null);
    setUnit(null);
    setPerLap("");
    setLapTxt("");
    setTank("");
  }, [key]);
  const num = (t: string) => {
    const v = Number(t.replace(",", "."));
    return t.trim() && Number.isFinite(v) && v > 0 ? v : null;
  };
  const overrides = { unit, per_lap: num(perLap), lap_time_s: parseLapTime(lapTxt), tank: num(tank) };
  const view = useAsync(
    () => api.raceStrategy(combo, length, margin, formation ? 0.5 : 0, overrides),
    [key, length?.kind, length?.value, margin, formation, unit, perLap, lapTxt, tank],
  );
  const v: RaceStrategyView | null = view.data ?? null;
  const u: FuelUnit = unit ?? v?.unit ?? "litres";
  const len = length ?? v?.default_length ?? { kind: "minutes", value: 30 };
  const s = v?.strategy;
  const energyPossible = combo.sim === "lmu" || v?.measured_unit === "energy";
  const longest = s ? Math.max(...s.stints.map((st) => st.to_lap - st.from_lap + 1)) : 0;
  const wear = v?.tyre_wear_per_lap;

  return (
    <div className="card fuelcalc">
      <div className="row" style={{ justifyContent: "space-between", alignItems: "center" }}>
        <h2 style={{ margin: 0 }}>{title}</h2>
        {energyPossible && (
          <div className="seg" role="group" aria-label="Unité">
            <button className={u === "energy" ? "on" : ""} onClick={() => (setUnit("energy"), setPerLap(""), setTank(""))} title="Hypercar et LMGT3 : le relais est limité par l'énergie virtuelle">Énergie %</button>
            <button className={u === "litres" ? "on" : ""} onClick={() => (setUnit("litres"), setPerLap(""), setTank(""))} title="Voitures sans énergie virtuelle (LMP2…) : le relais est limité par le carburant">Litres</button>
          </div>
        )}
      </div>

      <div className="fc-inputs">
        <label>
          <span>Course</span>
          <div className="row" style={{ gap: 6 }}>
            <div className="seg">
              <button className={len.kind === "minutes" ? "on" : ""} onClick={() => setLength({ kind: "minutes", value: len.kind === "minutes" ? len.value : 60 })}>min</button>
              <button className={len.kind === "laps" ? "on" : ""} onClick={() => setLength({ kind: "laps", value: len.kind === "laps" ? len.value : s?.laps ?? 20 })}>tours</button>
            </div>
            <input
              type="number"
              min={1}
              max={1440}
              value={len.value}
              aria-label={len.kind === "laps" ? "Nombre de tours" : "Durée en minutes"}
              onChange={(e) => {
                const n = Math.max(1, Number(e.target.value) || 1);
                setLength(len.kind === "laps" ? { kind: "laps", value: Math.round(n) } : { kind: "minutes", value: n });
              }}
            />
          </div>
          <small className="muted">{len.kind === "minutes" ? `${duration(len.value * 60)} de course` : " "}</small>
        </label>
        <label>
          <span>Conso par tour ({u === "energy" ? "%" : "L"})</span>
          <input type="text" inputMode="decimal" value={perLap} placeholder={v?.per_lap_measured != null ? n2(v.per_lap_measured) : "ex. 4,8"} onChange={(e) => setPerLap(e.target.value)} />
          <small className="muted">
            {v?.per_lap_measured != null ? `mesuré ${n2(v.per_lap_measured)}${v.per_lap_max != null ? `, max ${n2(v.per_lap_max)}` : ""} ${u === "energy" ? "%" : "L"} sur ${v.laps_used} tours` : "pas encore mesuré ici"}
          </small>
        </label>
        <label>
          <span>Temps au tour</span>
          <input type="text" value={lapTxt} placeholder={v?.lap_time_measured != null ? lapTime(v.lap_time_measured) : "ex. 3:31.4"} onChange={(e) => setLapTxt(e.target.value)} />
          <small className="muted">{v?.lap_time_measured != null ? `médiane de tes tours propres` : "pas encore mesuré ici"}</small>
        </label>
        <label>
          <span>{u === "energy" ? "Énergie au plein" : "Réservoir (L)"}</span>
          {u === "energy" ? (
            <input type="text" value="100 %" disabled aria-label="Énergie au plein" />
          ) : (
            <input type="text" inputMode="decimal" value={tank} placeholder={v?.tank_measured != null ? n1(v.tank_measured) : "ex. 75"} onChange={(e) => setTank(e.target.value)} />
          )}
          <small className="muted">{u === "energy" ? "réservoir d'énergie virtuelle" : v?.tank_measured != null ? "fiche de la voiture" : "inconnu : saisis-le"}</small>
        </label>
        <label>
          <span>Marge</span>
          <select value={margin} onChange={(e) => setMargin(Number(e.target.value))} aria-label="Marge de sécurité">
            {[0, 0.5, 1, 2].map((m) => <option key={m} value={m}>{m === 0 ? "aucune" : `${String(m).replace(".", ",")} tour${m > 1 ? "s" : ""}`}</option>)}
          </select>
          <small className="muted toolbar-check"><input type="checkbox" checked={formation} onChange={(e) => setFormation(e.target.checked)} /> tour de formation</small>
        </label>
      </div>

      {view.error && !s && <div className="muted" style={{ marginTop: 8 }}>{view.error}</div>}

      {s && v && (
        <>
          <div className="fc-head">
            <div className="fc-stops">
              <b>{s.stops}</b>
              <span>arrêt{s.stops > 1 ? "s" : ""}</span>
            </div>
            <p>{s.summary.split(" Ou économise")[0]}</p>
          </div>
          <div className="strategy-facts">
            <div><span>Tours à couvrir</span><b className="mono">{s.laps}</b></div>
            <div><span>{u === "energy" ? "Énergie totale" : "Carburant total"}</span><b className="mono">{amount(u, s.total)}</b></div>
            <div><span>Tours par plein</span><b className="mono">{s.laps_per_tank ?? "—"}</b></div>
            <div><span>Relais le plus long</span><b className="mono">{duration(longest * v.lap_time_s)}</b></div>
          </div>
          <table className="fc-stints">
            <thead>
              <tr><th>Relais</th><th>Tours</th><th className="num">Durée</th><th className="num">Partir avec</th></tr>
            </thead>
            <tbody>
              {s.stints.map((st, i) => (
                <tr key={i}>
                  <td>{i === 0 ? "Départ" : `Arrêt ${i}`}</td>
                  <td>{st.from_lap} à {st.to_lap} <span className="muted">({st.to_lap - st.from_lap + 1})</span></td>
                  <td className="num">{duration(st.duration_s)}</td>
                  <td className="num"><b className="mono">{amount(u, st.amount)}</b></td>
                </tr>
              ))}
            </tbody>
          </table>
          {s.window && s.stops > 0 && <div className="muted fc-note">Premier arrêt possible entre les tours {s.window[0]} et {s.window[1]}.</div>}
          {s.save_per_lap != null && <div className="fc-save">Un arrêt de moins si tu économises {u === "energy" ? `${n2(s.save_per_lap)} %` : `${n2(s.save_per_lap)} L`} par tour (lift & coast, passage de rapport plus tôt).</div>}
          {wear && (
            <div className="fc-tyres">
              <span className="muted">Pneus sur un relais complet ({s.laps_per_tank ?? longest} tours)</span>
              <div className="fc-tyre-grid">
                {wear.map((w, k) => {
                  const stint = w * (s.laps_per_tank ?? longest);
                  const worst = w === Math.max(...wear);
                  return (
                    <div key={k} className={worst ? "worst" : ""}>
                      <span>{WHEELS[k]}</span>
                      <b className="mono">−{n1(stint)} %</b>
                      <small className="muted">{n2(w)} %/tour</small>
                    </div>
                  );
                })}
              </div>
            </div>
          )}
          <div className="muted fc-note">
            {u === "energy"
              ? `Au stand, règle l'énergie virtuelle pour repartir avec le niveau indiqué ; le carburant suit le ratio carburant de ton réglage${v.fuel_per_lap_l != null ? ` (≈ ${n2(v.fuel_per_lap_l)} L par tour)` : ""}.`
              : "Au volant, dis « carburant » : le coach te dit s'il y a de quoi finir."}
          </div>
        </>
      )}
    </div>
  );
}
