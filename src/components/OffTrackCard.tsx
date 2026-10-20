import type { OffTrackZone } from "../lib/types";

/** Où le pilote sort de la piste, et sur quels tours : la première chose à corriger. */
export function OffTrackCard({ zones, onZone }: { zones: OffTrackZone[]; onZone?: (z: OffTrackZone) => void }) {
  if (zones.length === 0) return null;
  return (
    <div className="card offtrack">
      <h2>Sorties de piste</h2>
      <div className="offtrack-list">
        {zones.slice(0, 6).map((z) => (
          <button key={z.at_m} className="offtrack-zone" onClick={() => onZone?.(z)} disabled={!onZone}>
            <b>{z.corner ?? `à ${(z.at_m / 1000).toFixed(2)} km`}</b>
            <span className="muted">
              {z.laps.length} tour{z.laps.length > 1 ? "s" : ""} (n° {z.laps.join(", ")})
            </span>
          </button>
        ))}
      </div>
      <div className="muted" style={{ fontSize: 12, marginTop: 8 }}>
        Chaque sortie invalide le tour (et coûte un incident sur iRacing). Rester sur la piste à ces endroits passe avant la recherche de vitesse.
      </div>
    </div>
  );
}
