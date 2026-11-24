import { useState } from "react";
import { runRenderChecks, type RenderCheck } from "../lib/renderCheck";
import { Badge, Button, Card } from "./ui";

/** Réglages › Avancé : l'app vérifie elle-même son affichage (capture à envoyer si une ligne est en échec). */
export function RenderCheckCard() {
  const [res, setRes] = useState<RenderCheck[] | null>(null);
  const failed = res?.filter((r) => !r.ok).length ?? 0;
  return (
    <Card className="mb-16" title="Affichage" action={<Button onClick={() => setRes(runRenderChecks())}>Vérifier l'affichage</Button>}>
      {!res && <p className="muted m-0">Vérifie que ce PC affiche les couleurs, les bandeaux et les polices de l'app comme prévu.</p>}
      {res && (
        <>
          <p className="m-0">{failed === 0 ? "Tout s'affiche comme prévu." : `${failed} point${failed > 1 ? "s" : ""} en échec : envoie une capture de cet écran.`}</p>
          <table className="mt-8">
            <tbody>
              {res.map((r) => (
                <tr key={r.id}>
                  <td>{r.label}</td>
                  <td className="mono t-sm">{r.detail}</td>
                  <td className="num">{r.id === "engine" ? null : <Badge tone={r.ok ? "gain" : "danger"}>{r.ok ? "OK" : "Échec"}</Badge>}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </>
      )}
    </Card>
  );
}
