import { useState } from "react";
import { api } from "../lib/api";
import { delta, lapTime } from "../lib/format";
import { useAsync } from "../lib/hooks";
import type { StatusPayload } from "../lib/types";
import { coachVoice, coachVolume, setCoachVoice, setCoachVolume, speak, voiceAvailable, voiceName } from "../lib/voice";

const SAMPLE = "Radio check. Plus 3 dixièmes. Sur les 3 derniers tours, focus virage 16, secteur 3 : 3 dixièmes perdus. Freine 10 mètres plus tard.";

/**
 * Coach vocal en direct : à chaque passage de la ligne, synthèse des derniers tours contre la
 * référence et un seul point à travailler, prononcé comme à la radio.
 */
export function LiveCoachPage({ status }: { status: StatusPayload | null }) {
  const live = status?.live;
  const msgs = [...(status?.coach ?? [])].reverse();
  const [winVoice, setWinVoice] = useState<string | null>(voiceName());
  const voices = useAsync(() => api.ttsVoices(), []);
  const [sel, setSel] = useState(coachVoice());
  const [busy, setBusy] = useState<string | null>(null);
  const [vol, setVol] = useState(coachVolume());
  // Volume appliqué tout de suite (même au message en cours), enregistré au relâchement du curseur.
  const saveVolume = async (v: number) => {
    try {
      const s = await api.settings();
      await api.saveSettings({ ...s, coach_volume: v });
    } catch (e) {
      setBusy(e instanceof Error ? e.message : String(e));
    }
  };
  const choose = async (id: string) => {
    setSel(id);
    setBusy(null);
    try {
      const s = await api.settings();
      await api.saveSettings({ ...s, coach_voice: id });
      setCoachVoice(id);
    } catch (e) {
      setBusy(e instanceof Error ? e.message : String(e));
    }
  };
  const test = async () => {
    if (sel !== "windows" && !voices.data?.find((v) => v.id === sel)?.installed) {
      setBusy("Téléchargement de la voix (environ 80 Mo la première fois)…");
      try {
        await api.ttsInstall(sel);
        voices.reload();
      } catch (e) {
        setBusy(e instanceof Error ? e.message : String(e));
        return;
      }
    }
    setBusy(null);
    await speak(SAMPLE, sel);
    setTimeout(() => setWinVoice(voiceName()), 300);
  };
  return (
    <>
      <div className="page-title"><h1>Live Coach</h1><span className="chip warn">Nouveau : à valider en piste</span></div>
      <div className="grid cols-2">
        <div className="card">
          <h2>Radio</h2>
          {msgs.length === 0 && (
            <div className="muted">
              Lance une session iRacing : le coach annonce sa référence dès que tu es en piste, puis te parle à chaque passage de la ligne.
            </div>
          )}
          {msgs.map((m, i) => (
            <div key={`${m.lap}-${i}`} className={`radio-msg ${m.kind} ${i === 0 ? "latest" : ""}`}>
              <div className="row" style={{ justifyContent: "space-between" }}>
                <span className="muted">{m.lap > 0 ? `Tour ${m.lap}${m.lap_time_s ? ` · ${lapTime(m.lap_time_s)}` : ""}` : "Mise en route"}</span>
                {m.lap_delta_s != null && <span className={`mono ${m.lap_delta_s > 0 ? "pos" : "neg"}`}>{delta(m.lap_delta_s)} s</span>}
              </div>
              <div className="radio-text">{m.text}</div>
            </div>
          ))}
        </div>
        <div>
          <div className="card" style={{ marginBottom: 14 }}>
            <h2>Voix</h2>
            <div className="voice-list">
              {[{ id: "windows", label: "Windows (synthèse intégrée)", installed: true }, ...(voices.data ?? [])].map((v) => (
                <label key={v.id} className={`voice-opt ${sel === v.id ? "on" : ""}`}>
                  <input type="radio" name="voice" checked={sel === v.id} onChange={() => void choose(v.id)} />
                  <span>{v.label}</span>
                  <span className="muted" style={{ fontSize: 12 }}>{v.id === "windows" ? (winVoice ?? "voix par défaut") : v.installed ? "installée" : "naturelle · à télécharger"}</span>
                </label>
              ))}
            </div>
            <div className="row" style={{ marginTop: 10, gap: 10 }}>
              <span className="muted">Volume</span>
              <input
                type="range"
                min={0}
                max={100}
                step={5}
                value={vol}
                style={{ flex: 1 }}
                onChange={(e) => {
                  const v = Number(e.target.value);
                  setVol(v);
                  setCoachVolume(v);
                }}
                onPointerUp={() => void saveVolume(vol)}
                onKeyUp={() => void saveVolume(vol)}
              />
              <span className="mono" style={{ width: 44, textAlign: "right" }}>{vol} %</span>
            </div>
            <button className="btn primary" onClick={() => void test()} style={{ marginTop: 8 }}>{sel !== "windows" && !voices.data?.find((v) => v.id === sel)?.installed ? "Télécharger et tester" : "Tester la voix"}</button>
            {busy && <div className="muted" style={{ marginTop: 8 }}>{busy}</div>}
            {!voiceAvailable() && sel === "windows" && <div className="warning">Synthèse vocale Windows indisponible sur ce poste.</div>}
            <div className="muted" style={{ fontSize: 12, marginTop: 8 }}>
              Voix naturelles : synthèse neuronale hors ligne (Piper), téléchargée une fois puis utilisée sans connexion. Un seul message par tour, au passage de la ligne. Réglages : « Audio » coupe la voix, « Actif en course » la coupe en course.
            </div>
          </div>
          <div className="card" style={{ marginBottom: 14 }}>
            <h2>Connexion simulateur</h2>
            <div className="kpi" style={{ fontSize: 18 }}>{status?.label ?? "—"}</div>
            <div className="kpi-sub">Jeux détectés : {status?.running.length ? status.running.join(", ") : "aucun"}</div>
            {live && (
              <div className="grid cols-2 mono" style={{ marginTop: 8 }}>
                <div>Vitesse <b>{live.speed_kmh.toFixed(0)} km/h</b></div>
                <div>Rapport <b>{live.gear}</b></div>
                <div>Tour <b>{live.lap}</b> ({(live.lap_dist_pct * 100).toFixed(1)} %)</div>
                <div>En piste <b>{live.on_track ? "oui" : "non"}</b></div>
              </div>
            )}
          </div>
          <div className="card">
            <h2>Comment il juge</h2>
            <ul className="muted" style={{ margin: 0, paddingLeft: 18 }}>
              <li>Chaque tour est comparé à ta référence (perso ou Garage 61), sinon à ton meilleur tour propre de la séance.</li>
              <li>Synthèse des 3 derniers tours propres : le virage où tu perds le plus, et de façon répétée, devient le focus.</li>
              <li>Consigne tirée de la mesure : freiner, tourner ou accélérer plus tôt / plus tard. Sans cause nette : « vise la corde ».</li>
              <li>Le focus reste le même tant qu'il coûte du temps : on travaille un point à la fois.</li>
              <li>Tours avec sortie de piste ou incident : non comptés, la sortie est signalée.</li>
            </ul>
          </div>
        </div>
      </div>
    </>
  );
}
