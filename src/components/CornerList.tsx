import { useState } from "react";
import { cornerFacts } from "../lib/compareView";
import type { CornerComparison } from "../lib/types";

const m = (v: number | null | undefined) => (v == null ? "—" : `${v.toFixed(0)} m`);
const kmh = (v: number) => `${v.toFixed(0)} km/h`;

/**
 * Virage par virage : barre de temps perdu / gagné et, en clair, ce qui change par rapport à la référence.
 * Un clic choisit le virage (conseils, carte, courbes) et déplie le détail chiffré.
 */
export function CornerList({ corners, selected, onSelect, onRename }: {
  corners: CornerComparison[];
  selected: number | null;
  onSelect: (id: number | null) => void;
  onRename: (apexM: number, name: string) => void;
}) {
  const [renaming, setRenaming] = useState<{ apex: number; name: string } | null>(null);
  const max = Math.max(0.1, ...corners.map((k) => Math.abs(k.time_delta_s)));
  const save = () => {
    if (renaming) onRename(renaming.apex, renaming.name);
    setRenaming(null);
  };
  return (
    <div className="card corners">
      <div className="row" style={{ justifyContent: "space-between", alignItems: "baseline", marginBottom: 6 }}>
        <h2 style={{ margin: 0 }}>Virage par virage</h2>
        <span className="muted" style={{ fontSize: 12 }}>
          <i className="swatch loss" /> temps perdu <i className="swatch gain" /> temps gagné · clic : détail
        </span>
      </div>
      {corners.map((k) => {
        const d = k.time_delta_s;
        const facts = cornerFacts(k).slice(0, 2);
        const open = selected === k.corner.id;
        const w = `${Math.min(100, (Math.abs(d) / max) * 100)}%`;
        return (
          <div key={k.corner.id} className={`corner-row ${open ? "on" : ""}`} onClick={() => onSelect(open ? null : k.corner.id)}>
            <div className="corner-name" onClick={(e) => e.stopPropagation()}>
              {renaming?.apex === k.corner.apex_m ? (
                <input
                  autoFocus
                  type="text"
                  value={renaming.name}
                  maxLength={40}
                  placeholder="nom (vide = V…)"
                  onChange={(e) => setRenaming({ apex: k.corner.apex_m, name: e.target.value })}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") save();
                    if (e.key === "Escape") setRenaming(null);
                  }}
                  onBlur={save}
                />
              ) : (
                <span title="Renommer ce virage (gardé pour ce circuit)" onClick={() => setRenaming({ apex: k.corner.apex_m, name: /^V\d+$/.test(k.corner.name) ? "" : k.corner.name })}>
                  {k.corner.name} <span className="pen">✎</span>
                </span>
              )}
            </div>
            <div className="corner-bar" title={d > 0 ? "temps perdu dans ce virage" : "temps gagné dans ce virage"}>
              <span className="mid" />
              {Math.abs(d) >= 0.005 && <i className={d > 0 ? "loss" : "gain"} style={{ width: `calc(${w} / 2)` }} />}
            </div>
            <div className={`corner-delta mono ${d > 0.02 ? "pos" : d < -0.02 ? "neg" : "muted"}`}>
              {Math.abs(d) < 0.005 ? "±0.00" : `${d > 0 ? "+" : "−"}${Math.abs(d).toFixed(2)}`} s
            </div>
            <div className="corner-facts">
              {facts.length === 0 ? <span className="muted">comme la référence</span> : facts.map((f) => <span key={f.text} className={f.bad ? "fact bad" : "fact good"}>{f.text}</span>)}
            </div>
            {open && (
              <table className="corner-detail" onClick={(e) => e.stopPropagation()}>
                <thead>
                  <tr><th /><th className="num">Freinage</th><th className="num">Vitesse à la corde</th><th className="num">Remise des gaz</th><th className="num">Vitesse de sortie</th></tr>
                </thead>
                <tbody>
                  <tr><td><i className="swatch ref" />Référence</td><td className="num">{m(k.reference.brake_peak >= 0.2 ? k.reference.brake_on_m : null)}</td><td className="num">{kmh(k.reference.min_speed_kmh)}</td><td className="num">{m(k.reference.throttle_on_m)}</td><td className="num">{kmh(k.reference.exit_speed_kmh)}</td></tr>
                  <tr><td><i className="swatch cmp" />Toi</td><td className="num">{m(k.compared.brake_peak >= 0.2 ? k.compared.brake_on_m : null)}</td><td className="num">{kmh(k.compared.min_speed_kmh)}</td><td className="num">{m(k.compared.throttle_on_m)}</td><td className="num">{kmh(k.compared.exit_speed_kmh)}</td></tr>
                </tbody>
                {k.apex_offset_m != null && Math.abs(k.apex_offset_m) >= 0.3 && (
                  <tfoot><tr><td colSpan={5} className="muted">À la corde, tu passes {Math.abs(k.apex_offset_m).toFixed(1)} m plus à l'{k.apex_offset_m > 0 ? "intérieur" : "extérieur"} que la référence.</td></tr></tfoot>
                )}
              </table>
            )}
          </div>
        );
      })}
    </div>
  );
}
