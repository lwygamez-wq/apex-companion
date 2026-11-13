import { useEffect, useMemo, useState } from "react";
import { FuelCalculator } from "../components/FuelCalculator";
import { api } from "../lib/api";
import { useAsync } from "../lib/hooks";
import { convert, type ConvField } from "../lib/strategy";
import type { CircuitOverview } from "../lib/types";

type Combo = Pick<CircuitOverview, "sim" | "track" | "track_config" | "car" | "setup">;

const n1 = (v: number) => v.toFixed(1).replace(".", ",");
const n2 = (v: number) => v.toFixed(2).replace(".", ",");
const num = (t: string) => {
  const v = Number(t.replace(",", "."));
  return t.trim() && Number.isFinite(v) && v >= 0 ? v : null;
};
const keyOf = (c: Combo) => `${c.sim}|${c.track}|${c.track_config ?? ""}|${c.car}|${c.setup ?? ""}`;
/** Saisie libre : pas de séance, tout se tape à la main (énergie virtuelle LMU). */
const FREE: Combo = { sim: "lmu", track: "Saisie libre", track_config: null, car: "—", setup: null };

/**
 * Convertisseur du stand : énergie (%) ↔ carburant (l) ↔ tours, avec le ratio carburant.
 * On tape l'une des trois valeurs, les deux autres suivent. Ratio, consommation et réservoir sont
 * pré-remplis avec tes mesures sur ce circuit et cette voiture.
 */
function PitConverter({ combo }: { combo: Combo }) {
  const meas = useAsync(() => api.raceStrategy(combo, null, 0.5, 0, combo.sim === "lmu" ? { unit: "energy" } : null).catch(() => null), [keyOf(combo)]);
  const v = meas.data ?? null;
  const [from, setFrom] = useState<ConvField>("energy");
  const [txt, setTxt] = useState<Record<ConvField, string>>({ energy: "100", litres: "", laps: "" });
  const [ratioTxt, setRatioTxt] = useState("");
  const [perTxt, setPerTxt] = useState("");
  useEffect(() => {
    setRatioTxt("");
    setPerTxt("");
  }, [keyOf(combo)]);
  const ratio = num(ratioTxt) ?? v?.fuel_ratio_suggested ?? null;
  const per = num(perTxt) ?? (v?.unit === "energy" ? v.per_lap_measured ?? v.game_per_lap : null) ?? null;
  const tank = v?.fuel_tank_l ?? null;
  const r = ratio != null ? convert({ from, value: num(txt[from]) ?? 0, ratio, perLap: per, marginLaps: 0.5, tankL: tank }) : null;
  const shown = (f: ConvField) => {
    if (f === from || !r) return txt[f];
    if (f === "energy") return n1(r.energy);
    if (f === "litres") return n1(r.litres);
    return r.laps != null ? n1(r.laps) : "";
  };
  const edit = (f: ConvField) => (e: React.ChangeEvent<HTMLInputElement>) => {
    setFrom(f);
    setTxt((t) => ({ ...t, [f]: e.target.value }));
  };

  return (
    <div className="card pitconv">
      <h2 style={{ margin: 0 }}>Convertisseur du stand</h2>
      <p className="muted" style={{ margin: "4px 0 12px" }}>Tape l'énergie, les litres ou le nombre de tours : le reste suit. Carburant = énergie × ratio.</p>
      <div className="pc-grid">
        <label className={from === "energy" ? "src" : ""}>
          <span>Énergie</span>
          <div className="pc-in"><input inputMode="decimal" value={shown("energy")} onChange={edit("energy")} aria-label="Énergie virtuelle en %" /><i>%</i></div>
        </label>
        <span className="pc-op">×</span>
        <label>
          <span>Ratio</span>
          <div className="pc-in"><input inputMode="decimal" value={ratioTxt} placeholder={v?.fuel_ratio_suggested != null ? n2(v.fuel_ratio_suggested) : "0,62"} onChange={(e) => setRatioTxt(e.target.value)} aria-label="Ratio carburant" /><i>L/%</i></div>
          <small className="muted">{v?.fuel_ratio_suggested != null ? `conseillé ${n2(v.fuel_ratio_suggested)}` : "celui du jeu"}</small>
        </label>
        <span className="pc-op">=</span>
        <label className={from === "litres" ? "src" : ""}>
          <span>Carburant</span>
          <div className="pc-in"><input inputMode="decimal" value={shown("litres")} onChange={edit("litres")} aria-label="Carburant en litres" /><i>L</i></div>
          <small className="muted">{tank != null ? `réservoir ${Math.round(tank)} L` : " "}</small>
        </label>
        <span className="pc-op">→</span>
        <label className={from === "laps" ? "src" : ""}>
          <span>Tours</span>
          <div className="pc-in"><input inputMode="decimal" value={shown("laps")} onChange={edit("laps")} aria-label="Nombre de tours" /><i>tours</i></div>
          <small className="muted">marge 0,5 tour gardée</small>
        </label>
        <label>
          <span>Conso par tour</span>
          <div className="pc-in"><input inputMode="decimal" value={perTxt} placeholder={per != null && !perTxt ? n2(per) : "4,80"} onChange={(e) => setPerTxt(e.target.value)} aria-label="Consommation d'énergie par tour" /><i>%</i></div>
          <small className="muted">{v?.unit === "energy" && v.per_lap_measured != null ? `mesuré sur ${v.laps_used} tours` : v?.unit === "energy" && v.game_per_lap != null ? "estimation du jeu" : "pas encore mesuré"}</small>
        </label>
      </div>
      {ratio == null && <div className="muted" style={{ marginTop: 8 }}>Saisis le ratio réglé dans le jeu pour convertir.</div>}
      {r?.overFull && <div className="fc-warn">Plus de 100 % d'énergie : impossible. Au plus {per != null ? `${n1(100 / per - 0.5)} tours` : "100 %"} par relais.</div>}
      {r && !r.overFull && r.overTank && <div className="fc-warn">{n1(r.litres)} L dépassent le réservoir ({Math.round(tank ?? 0)} L) : avec ce ratio, tu ne peux pas mettre plus de {Math.floor(r.maxEnergy)} % d'énergie. C'est le carburant qui limite.</div>}
    </div>
  );
}

/** Onglet Stratégie : convertisseur du stand et plan de course (quoi mettre, barre de conso, arrêts). */
export function StrategyPage() {
  const circuits = useAsync(() => api.circuits(), []);
  const combos = useMemo(() => {
    const list = [...(circuits.data ?? [])].filter((c) => c.timed_laps > 0);
    // Le Mans Ultimate d'abord (simulateur principal), puis le plus récent.
    list.sort((a, b) => Number(b.sim === "lmu") - Number(a.sim === "lmu") || b.last_unix - a.last_unix);
    return list;
  }, [circuits.data]);
  // Voiture et circuit chargés dans le jeu : choisis d'office, même sans aucun tour roulé.
  const facts = useAsync(() => api.gameFacts().catch(() => null), []);
  const game: Combo | null = facts.data ? { sim: "lmu", track: facts.data.track, track_config: null, car: facts.data.car, setup: null } : null;
  const [sel, setSel] = useState<string | null>(null);
  const known = combos.filter((c) => !game || keyOf(c) !== keyOf(game));
  const combo: Combo = (game && (sel == null || sel === keyOf(game)) ? game : null) ?? combos.find((c) => keyOf(c) === sel) ?? (sel === keyOf(FREE) ? FREE : game ?? combos[0] ?? FREE);

  return (
    <div className="page strategy-page">
      <div className="page-title">
        <h1>Stratégie</h1>
        <select value={keyOf(combo)} onChange={(e) => setSel(e.target.value)} aria-label="Circuit et voiture" className="strategy-combo">
          {game && (
            <option value={keyOf(game)}>
              Dans le jeu · {game.track} · {game.car}
            </option>
          )}
          {known.map((c) => (
            <option key={keyOf(c)} value={keyOf(c)}>
              {c.sim === "lmu" ? "LMU" : "iRacing"} · {c.track}{c.track_config ? ` ${c.track_config}` : ""} · {c.car}
            </option>
          ))}
          <option value={keyOf(FREE)}>Saisie libre (LMU, sans séance)</option>
        </select>
      </div>
      {game && keyOf(combo) === keyOf(game) && facts.data && (
        <p className="muted strategy-game">
          Lu dans le jeu{facts.data.session >= 10 && facts.data.minutes != null ? ` : course de ${Math.round(facts.data.minutes)} min` : ""}
          {facts.data.energy_per_lap_pct != null ? ` · ${n2(facts.data.energy_per_lap_pct)} % d'énergie par tour annoncés` : facts.data.fuel_per_lap_l != null ? ` · ${n2(facts.data.fuel_per_lap_l)} L par tour annoncés` : ""}
          {facts.data.fuel_ratio != null ? ` · ratio réglé ${n2(facts.data.fuel_ratio)}` : ""}. Tes tours remplacent ces chiffres dès qu'ils sont mesurés.
        </p>
      )}
      <PitConverter combo={combo} />
      <FuelCalculator combo={combo} title="Plan de course" />
    </div>
  );
}
