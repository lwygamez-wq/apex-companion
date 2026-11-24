import type { StatusPayload } from "../lib/types";
import { Alert } from "./ui";

/** Alerte quand iRacing roule sans enregistrer la télémétrie (session perdue pour l'analyse). */
export function RecordingAlert({ status }: { status: StatusPayload | null }) {
  const active = status?.state.state === "session_active";
  if (!active || status?.live?.disk_logging !== false) return null;
  return (
    <Alert tone="danger" title="La télémétrie n'est pas enregistrée.">
      Appuie sur <b>Alt+L</b> dans iRacing, sinon cette session ne pourra pas être analysée. Astuce : active l'enregistrement automatique dans Réglages.
    </Alert>
  );
}
