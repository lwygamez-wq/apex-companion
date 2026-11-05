/**
 * Séance sans tour propre : les chiffres de rythme (meilleur tour propre, tour idéal, régularité)
 * restent vides. On dit pourquoi et ce qui compte à la prochaine séance, plutôt que des tirets muets.
 */
export function NoCleanLaps({ clean, timed }: { clean: number; timed: number }) {
  if (clean > 0 || timed === 0) return null;
  return (
    <div className="card notice hint">
      <div>
        <b>Aucun tour propre sur cette séance</b>
        <div className="muted">
          {timed} tour{timed > 1 ? "s" : ""}, aucun propre (sortie de piste, drapeau, stands ou tour incomplet). Le meilleur tour propre, le tour idéal et la
          régularité ne se calculent que sur des tours propres. Priorité de la prochaine séance : enchaîner 3 tours sans sortie.
        </div>
      </div>
    </div>
  );
}
