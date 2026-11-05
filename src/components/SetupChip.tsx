import type { SetupType } from "../lib/types";

/** Type de série : setup imposé (Fixed) ou libre (Open). Rien si inconnu, sauf `unknown`. */
export function SetupChip({ setup, unknown }: { setup: SetupType | null | undefined; unknown?: boolean }) {
  if (setup === "fixed") return <span className="chip setup-fixed" title="Série à setup imposé (Fixed)">Setup imposé</span>;
  if (setup === "open") return <span className="chip setup-open" title="Série à setup libre (Open)">Setup libre</span>;
  return unknown ? <span className="chip warn" title="Type de série inconnu : setup imposé ou libre ?">Setup ?</span> : null;
}

/** Choix du type de série d'une session (référence Garage 61 importée depuis un .csv). */
export function SetupSelect({ value, onChange }: { value: SetupType | null; onChange: (v: SetupType | null) => void }) {
  return (
    <select value={value ?? ""} onChange={(e) => onChange((e.target.value || null) as SetupType | null)} title="Le fichier Garage 61 ne dit pas si le tour a été roulé en série fixed ou open">
      <option value="">Série ?</option>
      <option value="fixed">Setup imposé</option>
      <option value="open">Setup libre</option>
    </select>
  );
}
