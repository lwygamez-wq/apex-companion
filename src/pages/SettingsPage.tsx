import { getVersion } from "@tauri-apps/api/app";
import { open, save } from "@tauri-apps/plugin-dialog";
import { useEffect, useState } from "react";
import { findUpdate, installUpdate } from "../components/UpdateBanner";
import { api, isTauri } from "../lib/api";
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

  const upd = <K extends keyof AppSettings>(k: K, v: AppSettings[K]) => setS((x) => (x ? { ...x, [k]: v } : x));
  const saveAll = async () => {
    if (!s) return;
    try {
      setS(await api.saveSettings(s));
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

      <div className="card" style={{ marginBottom: 14 }}>
        <h2>Simulateurs</h2>
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
              Installation : {c.install.detail} · Processus : {c.process_names.join(", ")}
              {c.capabilities.notes.map((n) => <div key={n}>• {n}</div>)}
            </div>
          </div>
        ))}
      </div>

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
            <div>Synchro automatique<div className="hint">Importe chaque nouveau .ibt dès qu'iRacing a fini de l'écrire, et rattrape au démarrage les sessions roulées app fermée.</div></div>
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
          <h2>Live Coach</h2>
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
          <button className="btn" onClick={() => void save({ defaultPath: "apex-export.json", filters: [{ name: "JSON", extensions: ["json"] }] }).then(async (p) => p && setMsg(`${await api.exportData(p)} session(s) exportée(s).`))}>Exporter toutes les données</button>
          <button className="btn" onClick={() => void api.diagnostics().then((d) => navigator.clipboard.writeText(JSON.stringify(d, null, 2))).then(() => setMsg("Diagnostic (sans données personnelles) copié dans le presse-papiers."))}>Copier le diagnostic</button>
          {!confirmDelete ? (
            <button className="btn danger" onClick={() => setConfirmDelete(true)}>Supprimer toutes les données…</button>
          ) : (
            <button className="btn danger" onClick={() => void api.deleteAll().then(() => { setConfirmDelete(false); setMsg("Toutes les données ont été supprimées."); })}>Confirmer la suppression définitive</button>
          )}
        </div>
      </div>
    </>
  );
}
