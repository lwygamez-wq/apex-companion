import { useEffect, useState } from "react";
import { api } from "../lib/api";
import { delta, lapTime } from "../lib/format";
import { useAsync } from "../lib/hooks";
import { RadioControlCard } from "../components/RadioControlCard";
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
  const [azKey, setAzKey] = useState("");
  const [azRegion, setAzRegion] = useState("francecentral");
  useEffect(() => {
    api.settings().then((s) => {
      setAzKey(s.azure_tts_key ?? "");
      setAzRegion(s.azure_tts_region ?? "francecentral");
    }).catch(() => undefined);
  }, []);
  const saveAzure = async () => {
    try {
      const s = await api.settings();
      await api.saveSettings({ ...s, azure_tts_key: azKey.trim() || null, azure_tts_region: azRegion });
      voices.reload();
      setBusy("Clé Azure enregistrée. Choisis une voix Azure puis « Tester la voix ».");
    } catch (e) {
      setBusy(e instanceof Error ? e.message : String(e));
    }
  };
  const test = async () => {
    if (sel.startsWith("azure:")) {
      if (!voices.data?.find((v) => v.id === sel)?.installed) {
        setBusy("Saisis d'abord ta clé et ta région Azure ci-dessous.");
        return;
      }
      setBusy(null);
      try {
        const wav = await api.ttsSpeak(sel, SAMPLE);
        const a = new Audio(URL.createObjectURL(new Blob([wav], { type: "audio/wav" })));
        a.volume = coachVolume() / 100;
        await a.play();
      } catch (e) {
        setBusy(e instanceof Error ? e.message : String(e));
      }
      return;
    }
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
  const all = [{ id: "windows", label: "Windows (synthèse intégrée)", installed: true }, ...(voices.data ?? [])];
  const groups: [string, typeof all][] = [
    ["Azure : les plus naturelles (en ligne)", all.filter((v) => v.id.startsWith("azure:"))],
    ["Hors ligne (téléchargées une fois)", all.filter((v) => !v.id.startsWith("azure:") && v.id !== "windows")],
    ["Windows", all.filter((v) => v.id === "windows")],
  ];
  const selInfo = all.find((v) => v.id === sel);
  const azureMissing = sel.startsWith("azure:") && !selInfo?.installed;
  return (
    <>
      <div className="page-title">
        <h1>Coach live</h1>
        <span className={`chip ${live?.on_track ? "good" : ""}`}>{status?.label ?? "—"}</span>
      </div>
      <div className="grid cols-2" style={{ alignItems: "start" }}>
        <div className="card radio-card">
          <h2>Radio</h2>
          {live && (
            <div className="live-strip mono">
              <span>{live.speed_kmh.toFixed(0)} km/h</span>
              <span>rapport {live.gear}</span>
              <span>tour {live.lap} · {(live.lap_dist_pct * 100).toFixed(0)} %</span>
            </div>
          )}
          {msgs.length === 0 && (
            <div className="muted">
              Lance une session iRacing ou Le Mans Ultimate : le coach annonce sa référence dès que tu es en piste, puis te parle à chaque passage de la ligne.
            </div>
          )}
          {msgs.map((m, i) => (
            <div key={`${m.lap}-${i}`} className={`radio-msg ${m.kind} ${i === 0 ? "latest" : ""}`}>
              <div className="row" style={{ justifyContent: "space-between" }}>
                <span className="muted">{m.kind === "reply" ? "Réponse" : m.kind === "callout" ? `Annonce · tour ${m.lap}` : m.lap > 0 ? `Tour ${m.lap}${m.lap_time_s ? ` · ${lapTime(m.lap_time_s)}` : ""}` : "Mise en route"}</span>
                {m.lap_delta_s != null && <span className={`mono ${m.lap_delta_s > 0 ? "pos" : "neg"}`}>{delta(m.lap_delta_s)} s</span>}
              </div>
              <div className="radio-text">{m.text}</div>
            </div>
          ))}
          <details className="how">
            <summary>Comment le coach juge</summary>
            <ul className="muted">
              <li>Chaque tour est comparé à ta référence de la même série (perso ou Garage 61), sinon à ton meilleur tour propre.</li>
              <li>Sur les 3 derniers tours propres, le virage où tu perds le plus devient le focus, et le reste tant qu'il coûte du temps.</li>
              <li>Consigne tirée de la mesure : freiner, tourner ou accélérer plus tôt / plus tard.</li>
              <li>Tours avec sortie de piste ou incident : non comptés, la sortie est signalée.</li>
            </ul>
          </details>
        </div>
        <div>
          <div className="card" style={{ marginBottom: 14 }}>
            <h2>Voix</h2>
            <div className="voice-row">
              <select value={sel} onChange={(e) => void choose(e.target.value)} aria-label="Voix du coach">
                {groups.filter((g) => g[1].length > 0).map(([g, list]) => (
                  <optgroup key={g} label={g}>
                    {list.map((v) => (
                      <option key={v.id} value={v.id}>
                        {v.label}{v.id.startsWith("azure:") ? (v.installed ? "" : " · clé requise") : v.id !== "windows" && !v.installed ? " · à télécharger" : ""}
                      </option>
                    ))}
                  </optgroup>
                ))}
              </select>
              <button className="btn primary" onClick={() => void test()}>{sel !== "windows" && !sel.startsWith("azure:") && !selInfo?.installed ? "Télécharger et tester" : "Tester"}</button>
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
                aria-label="Volume du coach"
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
            <details className="azure-box" open={azureMissing}>
              <summary>Clé Azure {voices.data?.some((v) => v.id.startsWith("azure:") && v.installed) ? "(enregistrée)" : ""}</summary>
              <div className="muted" style={{ fontSize: 12, margin: "6px 0" }}>
                portal.azure.com → ressource « Speech », niveau <b>Free F0</b>, région France Central → « Clés et point de terminaison » : copie la Clé 1. Seul le texte des messages radio est envoyé à Azure.
              </div>
              <div className="row" style={{ gap: 8 }}>
                <input type="password" placeholder="Clé 1" value={azKey} onChange={(e) => setAzKey(e.target.value)} style={{ flex: 1, minWidth: 200 }} />
                <select value={azRegion} onChange={(e) => setAzRegion(e.target.value)} aria-label="Région Azure">
                  {["francecentral", "westeurope", "northeurope", "swedencentral", "uksouth", "switzerlandnorth", "eastus", "westus2"].map((r) => <option key={r} value={r}>{r}</option>)}
                </select>
                <button className="btn" onClick={() => void saveAzure()}>Enregistrer</button>
              </div>
            </details>
            {busy && <div className="muted" style={{ marginTop: 8 }}>{busy}</div>}
            {!voiceAvailable() && sel === "windows" && <div className="warning">Synthèse vocale Windows indisponible sur ce poste.</div>}
            {sel === "windows" && winVoice && <div className="muted" style={{ fontSize: 12, marginTop: 6 }}>{winVoice}</div>}
          </div>
          <RadioControlCard status={status} />
        </div>
      </div>
    </>
  );
}
