import type { FuelUnit, RaceCandidate, RaceOptions } from "../lib/types";

const n1 = (v: number) => v.toFixed(1).replace(".", ",");
const stopsTxt = (k: number) => (k === 0 ? "Sans arrêt" : `${k} arrêt${k > 1 ? "s" : ""}`);
const set = (u: FuelUnit, v: number) => `${Math.ceil(v - 0.05)} ${u === "energy" ? "%" : "L"}`;

/** Rôle d'une option dans le verdict : « la plus rapide », « la plus sûre », « équivalente ». */
export function roleOf(o: RaceOptions, i: number): string | null {
  const v = o.verdict;
  if (v.kind === "clear") return i === v.best ? "la plus rapide" : null;
  if (v.kind === "equivalent") return i === v.safest ? "la plus sûre" : v.others.includes(i) ? "équivalente" : null;
  if (v.kind === "only") return i === v.best ? "seule sans économie" : null;
  return null;
}

/** Phrase de verdict, avec l'écart et son incertitude. */
export function verdictText(o: RaceOptions): string {
  const v = o.verdict;
  const k = (i: number) => stopsTxt(o.candidates[i]?.stops ?? 0);
  switch (v.kind) {
    case "clear":
      return `${k(v.best)} : ${Math.round(v.delta_s)} s de moins au stand que ${k(v.second).toLowerCase()} (± ${Math.round(2 * v.sd_s)} s).`;
    case "equivalent":
      return `Écart de ${Math.round(v.delta_s)} s dans l'incertitude (± ${Math.round(2 * v.sd_s)} s) : options équivalentes, ${k(v.safest).toLowerCase()} est la plus sûre.`;
    case "only":
      return `${k(v.best)} : seule option sans économie.`;
    case "unknown":
      return `Pas de comparaison : ${v.reason}.`;
  }
}

function Row({ c, u, role }: { c: RaceCandidate; u: FuelUnit; role: string | null }) {
  return (
    <tr className={role === "la plus rapide" || role === "la plus sûre" ? "ro-best" : ""}>
      <td>
        <b>{stopsTxt(c.stops)}</b>
        {c.saving > 0 && <span className="muted"> · économie {Math.round(c.saving * 100)} %</span>}
        {role && <div className="ro-role">{role}</div>}
      </td>
      <td className="num">{c.window && c.stops > 0 ? (c.window[0] < c.window[1] ? `T${c.window[0]}–T${c.window[1]}` : `T${c.window[1]}`) : "—"}</td>
      <td className="num">{c.fills.length ? c.fills.map((f) => set(u, f)).join(" · ") : "—"}</td>
      <td className="num">{c.pit_time_s == null ? "—" : `${Math.round(c.pit_time_s)} s ± ${Math.round(2 * (c.pit_time_sd_s ?? 0))}`}</td>
      <td className="num">{n1(c.arrival_laps)} tour</td>
      <td className="num">{c.wear_left_pct == null ? "—" : `${Math.round(c.wear_left_pct)} %`}</td>
      <td className="muted">{c.saving > 0 ? (c.breakeven_s_per_lap == null ? "coût non mesuré" : `rentable si le lift and coast coûte < ${n1(c.breakeven_s_per_lap)} s/tour`) : ""}</td>
    </tr>
  );
}

/** Comparatif des stratégies de course, sous le plan. */
export function RaceOptionsCard({ o }: { o: RaceOptions }) {
  if (!o.candidates.length) return null;
  return (
    <div className="ro-card">
      <h3>Options de course</h3>
      <p className="ro-verdict">{verdictText(o)}</p>
      <table className="su-table ro-table">
        <thead>
          <tr>
            <th>Option</th>
            <th>Arrêt</th>
            <th>À régler</th>
            <th>Au stand</th>
            <th>Arrivée</th>
            <th>Pneu le plus usé</th>
            <th></th>
          </tr>
        </thead>
        <tbody>
          {o.candidates.map((c, i) => (
            <Row key={`${c.stops}-${c.saving}`} c={c} u={o.unit} role={roleOf(o, i)} />
          ))}
        </tbody>
      </table>
      <p className="muted fc-note">
        {o.pit_model
          ? `Temps au stand mesuré sur tes arrêts : ${Math.round(o.pit_model.lane_s)} s de voie (± ${Math.round(2 * o.pit_model.lane_sd_s)}), ${o.pit_model.refill_s_per_unit.toFixed(2).replace(".", ",")} s par ${o.unit === "energy" ? "% remis" : "litre remis"}. `
          : ""}
        Fourchettes à deux écarts types. Non prévus : voiture de sécurité, drapeaux, incidents. Non mesuré : {o.missing.join(" ; ")}.
      </p>
    </div>
  );
}
