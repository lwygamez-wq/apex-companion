import type { StatusPayload } from "../lib/types";

/** Alerte quand iRacing roule sans enregistrer la télémétrie (session perdue pour l'analyse). */
export function RecordingAlert({ status }: { status: StatusPayload | null }) {
  const active = status?.state.state === "session_active";
  if (!active || status?.live?.disk_logging !== false) return null;
  return (
    <div className="error" style={{ display: "flex", justifyContent: "space-between", alignItems: "center", gap: 12 }}>
      <div>
        <b>La télémétrie n'est pas enregistrée.</b> Appuie sur <b>Alt+L</b> dans iRacing, sinon cette session ne pourra pas être analysée.
      </div>
      <span className="muted" style={{ fontSize: 12 }}>Astuce : active l'enregistrement automatique dans Réglages.</span>
    </div>
  );
}
