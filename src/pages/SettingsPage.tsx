import { getVersion } from "@tauri-apps/api/app";
import { open, save } from "@tauri-apps/plugin-dialog";
import { useEffect, useState } from "react";
import { findUpdate, installUpdate } from "../components/UpdateBanner";
import { api, isTauri } from "../lib/api";
import { setCoachVoice, setCoachVolume } from "../lib/voice";
import { SUPPORT_FR } from "../lib/format";
import { useAsync } from "../lib/hooks";
import type { AppSettings, Capabilities, Support } from "../lib/types";

const CAP_LABELS: [keyof Capabilities, string][] = [
  ["live_telemetry", "Télémétrie live"],
  ["post_session_import", "Import après session"],
  ["position_xy", "Position / trajectoire"],
  ["tyres", "Pneus"],
  ["damage", "Dégâts"],
  ["weather", "Météo"],
  ["replay", "Replay"],
];

const SUPPORT_CLS: Record<Support, string> = { implemented: "good", unverified: "warn", planned: "", unavailable: "bad" };

export function SettingsPage() {
  const settings = useAsync(() => api.settings(), []);
  const connectors = useAsync(() => api.connectors(), []);
  const autolog = useAsync(() => api.iracingAutolog(), []);
  const toggleAutolog = (enable: boolean) =>
    api.setIracingAutolog(enable).then(
      () => (setMsg(enable ? "Enregistrement automatique activé." : "Enregistrement automatique désactivé."), autolog.reload()),
      (e: unknown) => setMsg(e instanceof Error ? e.message : String(e)),
    );
  const [s, setS] = useState<AppSettings | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  const [confirmDelete, setConfirmDelete] = useState(false);
  useEffect(() => setS(settings.data), [settings.data]);
  const [version, setVersion] = useState<string>("");
  const [updMsg, setUpdMsg] = useState<string | null>(null);
  useEffect(() => {
    if (isTauri()) void getVersion().then(setVersion);
  }, []);
  const checkNow = async () => {
    setUpdMsg("Recherche…");
    try {
      const u = await findUpdate();
      if (!u) return setUpdMsg("Tu as déjà la dernière version.");
      setUpdMsg(`Version ${u.version} trouvée : téléchargement et vérification de la signature…`);
      await installUpdate();
    } catch (e) {
      setUpdMsg(e instanceof Error ? e.message : String(e));
    }
  };

  const [g61Msg, setG61Msg] = useState<string | null>(null);
  const checkG61 = async () => {
    if (!s) return;
    setG61Msg("Vérification…");
    try {
      await api.saveSettings(s);
      const who = await api.garage61Check();
      setG61Msg(`${who} Recherche des références pour tes sessions déjà roulées…`);
      const res = await api.findGarage61ReferencesAll();
      setG61Msg([who, ...res].join("\n"));
    } catch (e) {
      setG61Msg(e instanceof Error ? e.message : String(e));
    }
  };
  const recoverG61 = async () => {
    if (!s) return;
    setG61Msg("Récupération de tes sessions des 30 derniers jours sur Garage 61… (quelques minutes : une requête par tour, espacées pour respecter la limite de l'API)");
    try {
      await api.saveSettings(s);
      const r = await api.recoverGarage61Sessions(30);
      const head = r.sessions > 0 ? `${r.sessions} session(s) récupérée(s), ${r.laps} tours. Elles apparaissent dans Sessions et Historique.` : "Aucune session manquante à récupérer.";
      setG61Msg([head, ...r.messages].join("\n"));
    } catch (e) {
      setG61Msg(e instanceof Error ? e.message : String(e));
    }
  };
  const upd = <K extends keyof AppSettings>(k: K, v: AppSettings[K]) => setS((x) => (x ? { ...x, [k]: v } : x));
  const saveAll = async () => {
    if (!s) return;
    try {
      const saved = await api.saveSettings(s);
      setS(saved);
      setCoachVoice(saved.coach_voice);
      setCoachVolume(saved.coach_volume);
      setMsg("Réglages enregistrés.");
      connectors.reload();
    } catch (e) {
      setMsg(e instanceof Error ? e.message : String(e));
    }
  };

  return (
    <>
      <div className="page-title"><h1>Réglages</h1><button className="btn primary" onClick={() => void saveAll()} disabled={!s}>Enregistrer</button></div>
      {msg && <div className="warning">{msg}</div>}

      {s && (
        <div className="card" style={{ marginBottom: 14 }}>
          <h2>Télémétrie</h2>
          <div className="form-row">
            <div>Dossier de télémétrie iRacing<div className="hint">Vide = Documents\iRacing\telemetry</div></div>
            <div className="row">
              <input type="text" style={{ flex: 1, minWidth: 320 }} value={s.iracing_telemetry_dir ?? ""} placeholder="par défaut" onChange={(e) => upd("iracing_telemetry_dir", e.target.value || null)} />
              <button className="btn" onClick={() => void open({ directory: true }).then((d) => typeof d === "string" && upd("iracing_telemetry_dir", d))}>Parcourir</button>
            </div>
          </div>
          <div className="form-row">
            <div>
              Copie automatique pour le mobile
              <div className="hint">
                Chaque nouvelle session est exportée dans ce dossier, prête à ouvrir dans l'app mobile. Choisis ton dossier <b>iCloud Drive</b> (installe « iCloud pour Windows ») pour la retrouver sur ton iPhone. Vide = désactivé ; l'app n'envoie rien elle-même.
              </div>
            </div>
            <div className="row">
              <input type="text" style={{ flex: 1, minWidth: 320 }} value={s.mobile_export_dir ?? ""} placeholder="désactivé" onChange={(e) => upd("mobile_export_dir", e.target.value || null)} />
              <button className="btn" onClick={() => void open({ directory: true }).then((d) => typeof d === "string" && upd("mobile_export_dir", d))}>Parcourir</button>
            </div>
          </div>
          <div className="form-row">
            <div>
              Enregistrement automatique iRacing
              <div className="hint">Toutes les sessions sont enregistrées sans Alt+L (réglage irsdkAutoLogDisk d'iRacing, fichier sauvegardé avant modification). iRacing doit être fermé. Environ 200 Mo par heure.</div>
            </div>
            <div className="row">
              {autolog.error && <span className="muted">{autolog.error}</span>}
              {autolog.data && (
                <>
                  <span className={`chip ${autolog.data.enabled ? "good" : autolog.data.enabled === false ? "warn" : ""}`}>
                    {autolog.data.enabled ? "Actif" : autolog.data.enabled === false ? "Inactif" : "Réglage introuvable"}
                  </span>
                  {autolog.data.enabled != null && (
                    <button className="btn" onClick={() => void toggleAutolog(!autolog.data!.enabled)}>{autolog.data.enabled ? "Désactiver" : "Activer"}</button>
                  )}
                </>
              )}
            </div>
          </div>
          <div className="form-row">
            <div>Synchro automatique<div className="hint">Importe chaque nouveau .ibt dès qu'iRacing a fini de l'écrire, et rattrape au démarrage les sessions roulées app fermée. Le Mans Ultimate : Apex enregistre lui-même chaque passage en piste (Apex doit être ouvert pendant que tu roules).</div></div>
            <input type="checkbox" checked={s.auto_import} onChange={(e) => upd("auto_import", e.target.checked)} />
          </div>
          <div className="form-row">
            <div>Télémétrie live<div className="hint">Lecture de la mémoire partagée pendant la session.</div></div>
            <input type="checkbox" checked={s.live_enabled} onChange={(e) => upd("live_enabled", e.target.checked)} />
          </div>
          <div className="form-row">
            <div>Fréquence d'enregistrement live<div className="hint">60 Hz : points de freinage précis à ±1,5 m à 300 km/h. 20 Hz : ±4 m, CPU minimal.</div></div>
            <select value={s.sample_rate_hz} onChange={(e) => upd("sample_rate_hz", Number(e.target.value))}>
              {[20, 30, 60].map((h) => <option key={h} value={h}>{h} Hz</option>)}
            </select>
          </div>
          <div className="form-row">
            <div>Mode débutant<div className="hint">Affiche seulement tes 3 priorités et met la régularité avant la vitesse pure.</div></div>
            <input type="checkbox" checked={s.beginner_mode} onChange={(e) => upd("beginner_mode", e.target.checked)} />
          </div>
          <div className="form-row">
            <div>Unités</div>
            <select value={s.units} onChange={(e) => upd("units", e.target.value as AppSettings["units"])}>
              <option value="metric">Métriques (km/h, m)</option>
              <option value="imperial">Impériales (mph)</option>
            </select>
          </div>
        </div>
      )}

      {s && (
        <div className="card" style={{ marginBottom: 14 }}>
          <h2>Tours de référence (Garage 61)</h2>
          <div className="form-row">
            <div>
              Import depuis Téléchargements
              <div className="hint">Chaque tour exporté de Garage 61 (« Export CSV » sur le site) est importé tout seul dès qu'il arrive dans ton dossier Téléchargements, et devient ta référence sur ce circuit.</div>
            </div>
            <input type="checkbox" checked={s.watch_downloads} onChange={(e) => upd("watch_downloads", e.target.checked)} />
          </div>
          <div className="form-row">
            <div>Dossier de téléchargements<div className="hint">Vide = le dossier Téléchargements de Windows, détecté automatiquement même s'il a été déplacé (ex. D:\Téléchargements).</div></div>
            <div className="row">
              <input type="text" style={{ flex: 1, minWidth: 280 }} value={s.downloads_dir ?? ""} placeholder="détection automatique" onChange={(e) => upd("downloads_dir", e.target.value || null)} />
              <button className="btn" onClick={() => void open({ directory: true }).then((d) => typeof d === "string" && upd("downloads_dir", d))}>Parcourir</button>
            </div>
          </div>
          <div className="form-row">
            <div>
              Jeton d'accès Garage 61
              <div className="hint">
                Facultatif, pour la recherche entièrement automatique. Crée un jeton personnel sur garage61.net (menu Developer, droit « driving data ») et colle-le ici. Il reste sur ce PC. Garage 61 ne donne accès qu'aux tours des pilotes que tu suis et de tes coéquipiers : suis quelques pilotes rapides de ta voiture.
              </div>
            </div>
            <div className="row">
              <input type="password" style={{ flex: 1, minWidth: 280 }} value={s.garage61_token ?? ""} placeholder="non configuré" onChange={(e) => upd("garage61_token", e.target.value || null)} />
              <button className="btn" onClick={() => void checkG61()} disabled={!s.garage61_token} title="Vérifie le jeton puis cherche une référence pour chaque circuit/voiture déjà roulé">Vérifier et chercher</button>
              <button className="btn" onClick={() => void recoverG61()} disabled={!s.garage61_token} title="Recrée dans Apex tes sessions envoyées sur Garage 61 dont le fichier .ibt a disparu (30 derniers jours)">Récupérer mes sessions</button>
            </div>
          </div>
          {g61Msg && <div className="muted" style={{ padding: "4px 0 8px", whiteSpace: "pre-line" }}>{g61Msg}</div>}
          <div className="form-row">
            <div>
              Pilotes de référence
              <div className="hint">Un par ligne : colle l'adresse de la page de profil Garage 61 du pilote (ou son identifiant). Leurs meilleurs tours sont cherchés en plus de ceux des pilotes que tu suis.</div>
            </div>
            <textarea
              rows={4}
              style={{ width: "100%", background: "var(--panel-2)", border: "1px solid var(--line)", borderRadius: 4, padding: 8, color: "inherit", font: "inherit" }}
              value={(s.garage61_drivers ?? []).join("\n")}
              placeholder="https://garage61.net/…/luke-fleming"
              onChange={(e) => upd("garage61_drivers", e.target.value.split("\n").map((x) => x.trim()).filter(Boolean))}
            />
          </div>
          <div className="form-row">
            <div>Recherche automatique<div className="hint">Après chaque nouvelle session iRacing, et une fois par jour pour toutes tes sessions déjà roulées : cherche le tour le plus rapide disponible (même circuit, même voiture) et le prend comme référence s'il bat la référence actuelle.</div></div>
            <input type="checkbox" checked={s.garage61_auto} onChange={(e) => upd("garage61_auto", e.target.checked)} disabled={!s.garage61_token} />
          </div>
        </div>
      )}

      {s && (
        <div className="card" style={{ marginBottom: 14 }}>
          <h2>Coach live</h2>
          <div className="form-row"><div>Actif en course<div className="hint">Désactivé par défaut : entraînement et qualifications uniquement.</div></div><input type="checkbox" checked={s.coach_in_race} onChange={(e) => upd("coach_in_race", e.target.checked)} /></div>
          <div className="form-row"><div>Audio</div><input type="checkbox" checked={s.coach_audio} onChange={(e) => upd("coach_audio", e.target.checked)} /></div>
          <div className="form-row"><div>Intervalle minimum entre deux conseils (s)</div><input type="number" min={3} max={60} value={s.coach_min_interval_s} onChange={(e) => upd("coach_min_interval_s", Number(e.target.value))} style={{ width: 80 }} /></div>
        </div>
      )}

      {s && (
        <div className="card" style={{ marginBottom: 14 }}>
          <h2>Mises à jour</h2>
          <div className="form-row">
            <div>Version installée</div>
            <div className="mono">{version || "—"}</div>
          </div>
          <div className="form-row">
            <div>Vérifier au démarrage<div className="hint">Seule connexion internet de l'app : elle demande à GitHub s'il existe une version plus récente. Aucune donnée n'est envoyée.</div></div>
            <input type="checkbox" checked={s.check_updates} onChange={(e) => upd("check_updates", e.target.checked)} />
          </div>
          <div className="row" style={{ marginTop: 10 }}>
            <button className="btn" onClick={() => void checkNow()}>Rechercher une mise à jour</button>
            {updMsg && <span className="muted">{updMsg}</span>}
          </div>
        </div>
      )}

      <div className="card">
        <h2>Confidentialité et données</h2>
        <p className="muted">
          Toutes les données restent sur cet ordinateur (base SQLite locale). Aucune fonctionnalité cloud n'existe et aucune donnée n'est envoyée ; la seule connexion est la vérification des mises à jour, désactivable ci-dessus.
          Les journaux sont locaux et ne contiennent pas de télémétrie.
        </p>
        <div className="row">
          <button className="btn" onClick={() => void save({ defaultPath: "apex-export.json", filters: [{ name: "JSON", extensions: ["json"] }] })
            .then(async (p) => p && setMsg(`${await api.exportData(p)} session(s) exportée(s).`))
            .catch((e: unknown) => setMsg(`Export impossible : ${e instanceof Error ? e.message : String(e)}`))}>Exporter toutes les données</button>
          <button className="btn" onClick={() => void api.diagnostics().then((d) => navigator.clipboard.writeText(JSON.stringify(d, null, 2))).then(() => setMsg("Diagnostic (sans données personnelles) copié dans le presse-papiers."))
            .catch((e: unknown) => setMsg(`Copie impossible : ${e instanceof Error ? e.message : String(e)}`))}>Copier le diagnostic</button>
          {!confirmDelete ? (
            <button className="btn danger" onClick={() => setConfirmDelete(true)}>Supprimer toutes les données…</button>
          ) : (
            <button className="btn danger" onClick={() => void api.deleteAll().then(() => { setConfirmDelete(false); setMsg("Toutes les données ont été supprimées."); })}>Confirmer la suppression définitive</button>
          )}
        </div>
      </div>
      <details className="card" style={{ marginTop: 14 }}>
        <summary>Simulateurs pris en charge</summary>
        {(connectors.data ?? []).map((c) => (
          <div key={c.id} style={{ padding: "10px 0", borderBottom: "1px solid var(--line)" }}>
            <div className="row" style={{ justifyContent: "space-between" }}>
              <b>{c.name}</b>
              <span className="muted" style={{ fontSize: 12 }}>{c.capabilities.data_source}</span>
            </div>
            <div className="cap-grid">
              {CAP_LABELS.map(([k, label]) => {
                const v = c.capabilities[k] as Support;
                return <div className="cap" key={k}><span>{label}</span><span className={`chip ${SUPPORT_CLS[v]}`}>{SUPPORT_FR[v]}</span></div>;
              })}
            </div>
            <div className="muted" style={{ fontSize: 12 }}>
              <div>Installation : {c.install.detail}</div>
              <div>Processus détectés : {c.process_names.join(", ")}</div>
              {c.capabilities.notes.map((n) => <div key={n}>• {n}</div>)}
            </div>
          </div>
        ))}
      </details>

    </>
  );
}
