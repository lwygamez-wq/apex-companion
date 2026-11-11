import { useEffect, useState } from "react";
import { api } from "../lib/api";
import { lapTime } from "../lib/format";
import { useAsync } from "../lib/hooks";
import type { CircuitOverview, EngineerMode, FuelUnit, PlanOption, RaceLength, RaceStrategyView } from "../lib/types";

type Combo = Pick<CircuitOverview, "sim" | "track" | "track_config" | "car" | "setup">;

const n1 = (v: number) => v.toFixed(1).replace(".", ",");
const n2 = (v: number) => v.toFixed(2).replace(".", ",");
/** « 46,0 L » / « 87,4 % ». */
export const amount = (u: FuelUnit, v: number) => (u === "energy" ? `${n1(v)} %` : `${n1(v)} L`);
/** Consommation par tour : « 4,80 % » / « 2,35 L ». */
export const perLap = (u: FuelUnit, v: number) => (u === "energy" ? `${n2(v)} %` : `${n2(v)} L`);
/** Niveau à régler au stand, arrondi au-dessus : « 88 % » / « 46 L ». */
export const setting = (u: FuelUnit, v: number) => `${Math.ceil(v - 0.05)} ${u === "energy" ? "%" : "L"}`;
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

/** Économie acceptée par le plan auto pour faire un arrêt de moins (même seuil que l'ingénieur). */
const AUTO_SAVE = 0.04;

/** Plan affiché pour un mode de l'ingénieur : celui qu'il suivra en course avec ces chiffres. */
export function planFor(options: PlanOption[], mode: EngineerMode): PlanOption | null {
  const attack = options.find((o) => o.kind === "attack") ?? options[0] ?? null;
  const save = options.find((o) => o.kind === "save") ?? null;
  if (mode === "save") return save ?? attack;
  if (mode === "auto") return save && (save.saving ?? 1) <= AUTO_SAVE + 1e-6 ? save : attack;
  return attack;
}

const MODES: { mode: EngineerMode; label: string; hint: string }[] = [
  { mode: "auto", label: "Auto", hint: "un arrêt de moins si l'économie reste légère (4 % au plus), sinon on attaque" },
  { mode: "attack", label: "Attaque", hint: "le moins d'arrêts possible sans économiser" },
  { mode: "save", label: "Économie", hint: "un arrêt de moins tant que l'économie reste tenable (8 % au plus)" },
];

/**
 * Calculateur de relais : combien d'arrêts, quand, avec quel niveau repartir, quelle cible tenir par
 * tour. En % d'énergie virtuelle pour les voitures qui en ont (LMU : Hypercar, LMGT3), avec le ratio
 * carburant à régler au stand ; sinon en litres. Les valeurs mesurées sur tes tours propres sont
 * pré-remplies et peuvent être remplacées à la main. Le plan choisi est suivi par l'ingénieur en course.
 */
export function FuelCalculator({ combo, title = "Calculateur carburant / énergie" }: { combo: Combo; title?: string }) {
  const [length, setLength] = useState<RaceLength | null>(null);
  const [margin, setMargin] = useState(0.5);
  const [formation, setFormation] = useState(true);
  const [unit, setUnit] = useState<FuelUnit | null>(null);
  const [perLapTxt, setPerLapTxt] = useState("");
  const [lapTxt, setLapTxt] = useState("");
  const [tank, setTank] = useState("");
  const [ratioTxt, setRatioTxt] = useState("");
  const [mode, setMode] = useState<EngineerMode | null>(null);
  const [saveErr, setSaveErr] = useState<string | null>(null);
  const key = `${combo.sim}|${combo.track}|${combo.track_config}|${combo.car}|${combo.setup}`;
  // Autre circuit ou voiture : on repart des mesures.
  useEffect(() => {
    setLength(null);
    setUnit(null);
    setPerLapTxt("");
    setLapTxt("");
    setTank("");
    setRatioTxt("");
    setMode(null);
  }, [key]);
  const num = (t: string) => {
    const v = Number(t.replace(",", "."));
    return t.trim() && Number.isFinite(v) && v > 0 ? v : null;
  };
  const overrides = { unit, per_lap: num(perLapTxt), lap_time_s: parseLapTime(lapTxt), tank: num(tank), fuel_ratio: num(ratioTxt) };
  const view = useAsync(
    () => api.raceStrategy(combo, length, margin, formation ? 0.5 : 0, overrides),
    [key, length?.kind, length?.value, margin, formation, unit, perLapTxt, lapTxt, tank, ratioTxt],
  );
  const v: RaceStrategyView | null = view.data ?? null;
  const u: FuelUnit = unit ?? v?.unit ?? "litres";
  const len = length ?? v?.default_length ?? { kind: "minutes", value: 30 };
  const s = v?.strategy;
  const energyPossible = combo.sim === "lmu" || v?.measured_unit === "energy";
  const m: EngineerMode = mode ?? v?.engineer.mode ?? "auto";
  const chosen = s ? planFor(s.options, m) : null;
  const stints = chosen?.stints ?? s?.stints ?? [];
  const stops = chosen?.stops ?? s?.stops ?? 0;
  const longest = stints.length ? Math.max(...stints.map((st) => st.to_lap - st.from_lap + 1)) : 0;
  // Plan économie : plus de tours par plein à la cible ; plan attaque : à ta consommation.
  const perTank = s?.capacity != null && chosen?.kind === "save" ? Math.floor((s.capacity - margin * (v?.per_lap ?? 0)) / chosen.target_per_lap + 1e-4) : s?.laps_per_tank ?? null;
  const wear = v?.tyre_wear_per_lap;
  const ratio = s?.fuel_ratio ?? null;

  const follow = (next: EngineerMode, marginLaps = margin) => {
    setMode(next);
    setSaveErr(null);
    api.setEngineerPlan(combo, { mode: next, margin_laps: marginLaps }).catch((e: unknown) => setSaveErr(e instanceof Error ? e.message : String(e)));
  };

  let headline = s?.summary.split(" Ou économise")[0] ?? "";
  if (s && chosen?.kind === "save") {
    const w = chosen.window;
    headline = `${chosen.stops} arrêt${chosen.stops > 1 ? "s" : ""} si tu tiens ${perLap(u, chosen.target_per_lap)} par tour (lift and coast, ${Math.round((chosen.saving ?? 0) * 100)} % de moins)${w ? ` ; premier arrêt entre les tours ${w[0]} et ${w[1]}` : ""}.`;
  }

  return (
    <div className="card fuelcalc">
      <div className="row" style={{ justifyContent: "space-between", alignItems: "center" }}>
        <h2 style={{ margin: 0 }}>{title}</h2>
        {energyPossible && (
          <div className="seg" role="group" aria-label="Unité">
            <button className={u === "energy" ? "on" : ""} onClick={() => (setUnit("energy"), setPerLapTxt(""), setTank(""))} title="Hypercar et LMGT3 : le relais est limité par l'énergie virtuelle">Énergie %</button>
            <button className={u === "litres" ? "on" : ""} onClick={() => (setUnit("litres"), setPerLapTxt(""), setTank(""))} title="Voitures sans énergie virtuelle (LMP2…) : le relais est limité par le carburant">Litres</button>
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
          <small className="muted">{len.kind === "minutes" ? `${duration(len.value * 60)} de course, + 1 tour compté` : " "}</small>
        </label>
        <label>
          <span>Conso par tour ({u === "energy" ? "%" : "L"})</span>
          <input type="text" inputMode="decimal" value={perLapTxt} placeholder={v?.per_lap_measured != null ? n2(v.per_lap_measured) : "ex. 4,8"} onChange={(e) => setPerLapTxt(e.target.value)} />
          <small className="muted">
            {v?.per_lap_measured != null ? `mesuré ${n2(v.per_lap_measured)}${v.per_lap_max != null ? `, max ${n2(v.per_lap_max)}` : ""} ${u === "energy" ? "%" : "L"} sur ${v.laps_used} tours` : "pas encore mesuré ici"}
          </small>
        </label>
        <label>
          <span>Temps au tour</span>
          <input type="text" value={lapTxt} placeholder={v?.lap_time_measured != null ? lapTime(v.lap_time_measured) : "ex. 3:31.4"} onChange={(e) => setLapTxt(e.target.value)} />
          <small className="muted">{v?.lap_time_measured != null ? `médiane de tes tours propres` : "pas encore mesuré ici"}</small>
        </label>
        {u === "energy" ? (
          <label>
            <span>Ratio carburant (L par 1 %)</span>
            <input type="text" inputMode="decimal" value={ratioTxt} placeholder={v?.fuel_ratio_suggested != null ? n2(v.fuel_ratio_suggested) : "ex. 0,86"} onChange={(e) => setRatioTxt(e.target.value)} aria-label="Ratio carburant" />
            <small className="muted">
              {v?.fuel_ratio_measured != null ? `mesuré ${n2(v.fuel_ratio_measured)}, conseillé ${n2(v.fuel_ratio_suggested ?? v.fuel_ratio_measured)}` : "pas encore mesuré : saisis celui du jeu"}
              {v?.fuel_tank_l != null ? ` · réservoir ${Math.round(v.fuel_tank_l)} L` : ""}
            </small>
          </label>
        ) : (
          <label>
            <span>Réservoir (L)</span>
            <input type="text" inputMode="decimal" value={tank} placeholder={v?.tank_measured != null ? n1(v.tank_measured) : "ex. 75"} onChange={(e) => setTank(e.target.value)} />
            <small className="muted">{v?.tank_measured != null ? "fiche de la voiture" : "inconnu : saisis-le"}</small>
          </label>
        )}
        <label>
          <span>Marge</span>
          <select
            value={margin}
            onChange={(e) => {
              const mg = Number(e.target.value);
              setMargin(mg);
              follow(m, mg);
            }}
            aria-label="Marge de sécurité"
          >
            {[0, 0.5, 1, 2].map((x) => <option key={x} value={x}>{x === 0 ? "aucune" : `${String(x).replace(".", ",")} tour${x > 1 ? "s" : ""}`}</option>)}
          </select>
          <small className="muted toolbar-check"><input type="checkbox" checked={formation} onChange={(e) => setFormation(e.target.checked)} /> tour de formation</small>
        </label>
      </div>

      {view.error && !s && <div className="muted" style={{ marginTop: 8 }}>{view.error}</div>}

      {s && v && chosen && (
        <>
          <div className="fc-plan" role="group" aria-label="Plan suivi en course">
            <span className="muted">Plan suivi en course</span>
            <div className="seg">
              {MODES.map((x) => (
                <button key={x.mode} className={m === x.mode ? "on" : ""} onClick={() => follow(x.mode)} title={x.hint}>{x.label}</button>
              ))}
            </div>
            <small className="muted">{MODES.find((x) => x.mode === m)?.hint}.</small>
          </div>
          {saveErr && <div className="error" style={{ marginTop: 6 }}>{saveErr}</div>}

          <div className="fc-head">
            <div className="fc-stops">
              <b>{stops}</b>
              <span>arrêt{stops > 1 ? "s" : ""}</span>
            </div>
            <p>{headline}</p>
          </div>
          {s.options.length > 1 && (
            <div className="fc-options">
              {s.options.map((o) => (
                <div key={o.kind} className={`fc-opt ${o === chosen ? "on" : ""}`}>
                  <b>{o.kind === "attack" ? "Attaque" : "Économie"} · {o.stops} arrêt{o.stops > 1 ? "s" : ""}</b>
                  <span>{o.kind === "attack" ? `jusqu'à ${perLap(u, o.target_per_lap)} par tour` : `cible ${perLap(u, o.target_per_lap)} par tour, ${Math.round((o.saving ?? 0) * 100)} % de moins`}</span>
                </div>
              ))}
            </div>
          )}
          <div className="strategy-facts">
            <div><span>Tours à couvrir</span><b className="mono">{s.laps}</b></div>
            <div><span>{chosen.kind === "save" ? "Cible par tour" : "Conso max tenable"}</span><b className="mono">{perLap(u, chosen.target_per_lap)}</b></div>
            <div><span>Tours par plein</span><b className="mono">{perTank ?? "—"}</b></div>
            <div><span>Relais le plus long</span><b className="mono">{duration(longest * v.lap_time_s)}</b></div>
          </div>
          {s.fuel_limited && v.fuel_tank_l != null && ratio != null && (
            <div className="fc-warn">
              Avec un ratio de {n2(ratio)}, le réservoir de {Math.round(v.fuel_tank_l)} L ne contient que {Math.floor(v.fuel_tank_l / ratio)} % d'énergie : c'est le carburant qui limite les relais.
            </div>
          )}
          <table className="fc-stints">
            <thead>
              <tr><th>Relais</th><th>Tours</th><th className="num">Durée</th><th className="num">{u === "energy" ? "Régler au stand" : "Partir avec"}</th></tr>
            </thead>
            <tbody>
              {stints.map((st, i) => (
                <tr key={i}>
                  <td>{i === 0 ? "Départ" : `Arrêt ${i}`}</td>
                  <td>{st.from_lap} à {st.to_lap} <span className="muted">({st.to_lap - st.from_lap + 1})</span></td>
                  <td className="num">{duration(st.duration_s)}</td>
                  <td className="num">
                    <b className="mono">{setting(u, st.amount)}</b>
                    {u === "energy" && ratio != null && st.fuel_l != null && <span className="muted mono"> · ratio {n2(ratio)} · {Math.round(st.fuel_l)} L</span>}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          {chosen.window && stops > 0 && <div className="muted fc-note">Premier arrêt possible entre les tours {chosen.window[0]} et {chosen.window[1]}.</div>}
          {chosen.kind === "attack" && s.save_per_lap != null && (
            <div className="fc-save">Un arrêt de moins si tu économises {perLap(u, s.save_per_lap)} par tour (lift and coast avant les gros freinages, passage de rapport plus tôt) : plan Économie.</div>
          )}
          {wear && (
            <div className="fc-tyres">
              <span className="muted">Pneus sur un relais complet ({perTank ?? longest} tours)</span>
              <div className="fc-tyre-grid">
                {wear.map((w, k) => {
                  const stint = w * (perTank ?? longest);
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
              ? `Au stand, règle l'énergie virtuelle et le ratio : le carburant mis = énergie × ratio${ratio != null ? ` (${n2(ratio)} L par %)` : ""}. Un ratio trop bas fait lâcher le carburant avant l'énergie ; trop haut, la voiture est plus lourde et l'arrêt plus long. `
              : ""}
            En course, l'ingénieur annonce le relais, la cible par tour, quand lever le pied ou attaquer, et t'appelle au stand. Dis « stratégie » pour l'entendre.
          </div>
        </>
      )}
    </div>
  );
}
