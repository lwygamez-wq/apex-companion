import { api } from "../lib/api";
import { useAsync } from "../lib/hooks";
import { Card } from "./ui";

/**
 * Plan d'attaque de la prochaine séance (analysis/attack.rs) : 3 virages au plus, chacun avec un
 * repère chiffré tiré de ton meilleur passage propre. Rien n'est affiché sans repère mesuré.
 */
export function AttackCard({ sessionId, onMap }: { sessionId: number; onMap?: () => void }) {
  const plan = useAsync(() => api.attackPlan(sessionId).catch(() => []), [sessionId]);
  const items = plan.data ?? [];
  if (!items.length) return null;
  return (
    <Card className="attack-card mb-16" title="Plan d'attaque : tes repères pour la prochaine séance" action={onMap && <button className="btn small" onClick={onMap}>Voir sur la carte</button>}>
      <ol className="attack-list">
        {items.map((i) => (
          <li key={i.corner}>
            <b>{i.corner}</b>
            <span>{i.text.slice(i.corner.length + 3)}</span>
          </li>
        ))}
      </ol>
      <p className="muted t-sm m-0">Repères tirés de ton meilleur passage propre de la séance, comparé à ta médiane. Distances mesurées depuis la ligne de départ : aucun panneau de piste n'est connu dans les données.</p>
    </Card>
  );
}
