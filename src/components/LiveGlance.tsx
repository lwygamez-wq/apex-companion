// « Au volant » : ce qui se lit d'un coup d'œil, à distance de volant. Trois informations au plus,
// chiffres à 48 px, contraste maximal, aucune animation. Données inchangées : relais en cours et
// consigne de l'ingénieur (status.fuel_live), état de la source live.
import type { LiveFuel, StatusPayload } from "../lib/types";
import { amount } from "./FuelCalculator";

const n1 = (v: number) => v.toFixed(1).replace(".", ",");

export type GlanceTone = "neutral" | "gain" | "loss" | "danger" | "accent";
export interface Tile {
  label: string;
  value: string;
  sub: string;
  tone: GlanceTone;
}

/** Les trois tuiles : consigne, réserve, tours restants. */
export function glanceTiles(f: LiveFuel | null | undefined, lap: number | null): Tile[] {
  if (!f) return [];
  const u = f.unit;
  const e = f.engineer;
  let consigne: Tile;
  if (e && e.box_lap != null) {
    const soon = e.laps_to_box != null && e.laps_to_box <= 1;
    consigne = { label: "Ingénieur", value: soon ? "Box ce tour" : `Box fin T${e.box_lap}`, sub: e.laps_to_box != null && !soon ? `dans ${e.laps_to_box} tours` : "prépare le MFD", tone: soon ? "accent" : "neutral" };
  } else if (e) {
    const lift = e.verdict === "lift";
    consigne = { label: "Ingénieur", value: lift ? "Lift and coast" : "Dans la cible", sub: `cible ${amount(u, e.target_per_lap)} par tour`, tone: lift ? "loss" : "gain" };
  } else if (f.laps_left != null && f.need_to_finish != null) {
    const ok = f.level >= f.need_to_finish;
    consigne = { label: "Pour finir", value: ok ? "De quoi finir" : "Arrêt nécessaire", sub: ok ? `marge ${amount(u, f.level - f.need_to_finish)}` : `il manque ${amount(u, f.need_to_finish - f.level)}`, tone: ok ? "gain" : "danger" };
  } else {
    consigne = { label: "Ingénieur", value: "—", sub: "consigne après le premier tour lancé", tone: "neutral" };
  }
  const reserve: Tile = { label: u === "energy" ? "Énergie" : "Carburant", value: amount(u, f.level), sub: f.laps_in_tank != null ? `${n1(f.laps_in_tank)} tours possibles` : "conso mesurée au 1er tour", tone: "neutral" };
  const left: Tile = f.laps_left != null ? { label: "Tours restants", value: n1(f.laps_left), sub: "course", tone: "neutral" } : { label: "Tour", value: lap != null ? String(lap) : "—", sub: "hors course", tone: "neutral" };
  return [consigne, reserve, left];
}

/** État de la source live, en clair. */
export function liveState(s: StatusPayload | null): { text: string; tone: "gain" | "loss" | "neutral" } {
  if (!s) return { text: "Démarrage…", tone: "neutral" };
  switch (s.state.state) {
    case "session_active":
      return { text: s.live?.on_track ? "En piste" : "Connecté, au stand", tone: "gain" };
    case "waiting_for_session":
    case "game_detected":
      return { text: s.label, tone: "loss" };
    default:
      return { text: s.label, tone: "neutral" };
  }
}

export function LiveGlance({ status }: { status: StatusPayload | null }) {
  const tiles = glanceTiles(status?.fuel_live, status?.live?.lap ?? null);
  const st = liveState(status);
  return (
    <section className="glance" aria-label="Au volant, en un coup d'œil">
      <div className={`glance-state tone-${st.tone}`} role="status">
        <i aria-hidden="true" />
        {st.text}
      </div>
      {tiles.length === 0 ? (
        <div className="glance-wait">Lance Le Mans Ultimate (ou iRacing) et sors en piste : la consigne, la réserve et les tours restants s'affichent ici en grand.</div>
      ) : (
        <div className="glance-tiles">
          {tiles.map((t) => (
            <div key={t.label} className={`glance-tile tone-${t.tone}`}>
              <span className="glance-label">{t.label}</span>
              <b className="glance-value">{t.value}</b>
              <span className="glance-sub">{t.sub}</span>
            </div>
          ))}
        </div>
      )}
    </section>
  );
}
