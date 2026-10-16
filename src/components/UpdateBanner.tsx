import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { api, isTauri } from "../lib/api";

export interface UpdateInfo {
  version: string;
  notes: string | null;
}

const errMsg = (e: unknown) => (typeof e === "string" ? e : e instanceof Error ? e.message : String(e));

/** Demande à GitHub s'il existe une version plus récente. `null` = déjà à jour. */
export async function findUpdate(): Promise<UpdateInfo | null> {
  if (!isTauri()) return null;
  try {
    return await invoke<UpdateInfo | null>("check_update");
  } catch (e) {
    throw new Error(errMsg(e));
  }
}

/** Télécharge l'installeur, vérifie sa signature, le lance et ferme l'app. Les données sont conservées. */
export async function installUpdate(): Promise<void> {
  try {
    await invoke<string>("install_update");
  } catch (e) {
    throw new Error(errMsg(e));
  }
}

/** Bandeau affiché au démarrage quand une nouvelle version est disponible. */
export function UpdateBanner() {
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [hidden, setHidden] = useState(false);

  useEffect(() => {
    api
      .settings()
      .then((s) => (s.check_updates ? findUpdate() : null))
      .then(setUpdate)
      // Hors ligne ou GitHub indisponible : on ne dérange pas l'utilisateur.
      .catch(() => undefined);
  }, []);

  if (!update || hidden) return null;
  return (
    <div className="update-banner">
      <div>
        <b>Nouvelle version {update.version} disponible</b>
        {update.notes && <div className="muted" style={{ fontSize: 12, marginTop: 2 }}>{update.notes}</div>}
        {error && <div className="pos" style={{ fontSize: 12, marginTop: 4 }}>Échec : {error}</div>}
      </div>
      <div className="row">
        {!busy && <button className="btn" onClick={() => setHidden(true)}>Plus tard</button>}
        <button
          className="btn primary"
          disabled={busy}
          onClick={() => {
            setError(null);
            setBusy(true);
            installUpdate().catch((e: unknown) => {
              setBusy(false);
              setError(errMsg(e));
            });
          }}
        >
          {busy ? "Téléchargement et vérification…" : "Installer la mise à jour"}
        </button>
      </div>
    </div>
  );
}
