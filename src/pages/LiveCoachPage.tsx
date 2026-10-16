import type { StatusPayload } from "../lib/types";

/**
 * Live Coach — phase 2. Cet écran affiche uniquement ce qui fonctionne réellement :
 * le statut de connexion et les données live brutes (iRacing, Windows, non validé).
 * Aucun conseil live n'est simulé.
 */
export function LiveCoachPage({ status }: { status: StatusPayload | null }) {
  const live = status?.live;
  return (
    <>
      <div className="page-title"><h1>Live Coach</h1><span className="chip warn">Phase 2 — en développement</span></div>
      <div className="grid cols-2">
        <div className="card">
          <h2>Connexion simulateur</h2>
          <div className="kpi" style={{ fontSize: 18 }}>{status?.label ?? "—"}</div>
          <div className="kpi-sub">Jeux détectés : {status?.running.length ? status.running.join(", ") : "aucun"}</div>
          {status?.last_auto_import && <div className="kpi-sub">Dernier import auto : {status.last_auto_import}</div>}
        </div>
        <div className="card">
          <h2>Données live essentielles</h2>
          {!live && <div className="muted">Aucune source live connectée. Seul iRacing (Windows) est câblé à ce stade, en mode non validé.</div>}
          {live && (
            <div className="grid cols-2 mono">
              <div>Vitesse <b>{live.speed_kmh.toFixed(0)} km/h</b></div>
              <div>Rapport <b>{live.gear}</b></div>
              <div>Tour <b>{live.lap}</b> ({(live.lap_dist_pct * 100).toFixed(1)} %)</div>
              <div>En piste <b>{live.on_track ? "oui" : "non"}</b></div>
              <div>Gaz <b>{(live.throttle * 100).toFixed(0)} %</b></div>
              <div>Frein <b>{(live.brake * 100).toFixed(0)} %</b></div>
              <div>Enregistrement <b>{live.disk_logging == null ? "inconnu" : live.disk_logging ? "actif" : "INACTIF (Alt+L)"}</b></div>
            </div>
          )}
        </div>
      </div>
      <div className="card" style={{ marginTop: 14 }}>
        <h2>Ce qui arrive en phase 2</h2>
        <ul className="muted">
          <li>Delta live par distance contre la référence choisie (même moteur que l'analyse post-session).</li>
          <li>Indication du prochain freinage (tôt / cohérent / tard) seulement si la confiance est élevée sur ce virage.</li>
          <li>Affichage sur second écran, tablette ou téléphone via le réseau local — plus robuste qu'un overlay au-dessus d'un triple écran en plein écran.</li>
          <li>Règles anti-distraction : fréquence limitée, aucun message pendant freinage ou appui, désactivé par défaut en course.</li>
        </ul>
      </div>
    </>
  );
}
