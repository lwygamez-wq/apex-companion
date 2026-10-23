import { useEffect, useState } from "react";
import { api, onPtt } from "../lib/api";
import type { AppSettings, PttEvent, StatusPayload } from "../lib/types";

const COMMANDS: [string, string][] = [
  ["répète", "redit le dernier conseil"],
  ["point", "dernier tour, meilleur tour, focus"],
  ["silence", "coupe les messages automatiques"],
  ["reprends", "remet la radio"],
  ["plan", "rappelle les objectifs de la séance"],
];

function bindingLabel(s: AppSettings | null): string {
  const b = s?.ptt_binding;
  if (!b) return "aucun";
  return b.type === "joystick" ? `${b.name}, bouton ${b.button + 1}` : `touche ${b.name}`;
}

/** Bouton radio du volant + mots-clés vocaux, et réglages des annonces. */
export function RadioControlCard({ status }: { status: StatusPayload | null }) {
  const [s, setS] = useState<AppSettings | null>(null);
  const [installed, setInstalled] = useState<boolean | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  const [ptt, setPtt] = useState<PttEvent | null>(null);

  useEffect(() => {
    api.settings().then(setS).catch(() => undefined);
    api.voiceInstalled().then(setInstalled).catch(() => setInstalled(false));
    const un = onPtt(setPtt);
    return () => {
      void un.then((f) => f());
    };
  }, []);

  const save = async (patch: Partial<AppSettings>) => {
    if (!s) return;
    try {
      setS(await api.saveSettings({ ...s, ...patch }));
    } catch (e) {
      setMsg(e instanceof Error ? e.message : String(e));
    }
  };
  const learn = async () => {
    setMsg("Appuie sur le bouton du volant à utiliser pour la radio (10 s)…");
    try {
      const b = await api.pttLearn();
      setS(await api.settings());
      setMsg(b ? "Bouton enregistré. Maintiens-le et dis un mot-clé." : "Aucun bouton détecté. S'il ne réagit pas, associe-le à une touche clavier (SimHub, logiciel du volant) et recommence.");
    } catch (e) {
      setMsg(e instanceof Error ? e.message : String(e));
    }
  };
  const install = async () => {
    setMsg("Téléchargement de la reconnaissance vocale (environ 55 Mo)…");
    try {
      await api.voiceInstall();
      setInstalled(true);
      setMsg("Reconnaissance vocale installée.");
    } catch (e) {
      setMsg(e instanceof Error ? e.message : String(e));
    }
  };

  return (
    <div className="card" style={{ marginBottom: 14 }}>
      <div className="row" style={{ justifyContent: "space-between" }}>
        <h2>Commande au volant</h2>
        {status?.muted ? (
          <button className="btn" onClick={() => void api.setCoachMuted(false)}>Radio coupée : reprendre</button>
        ) : (
          <button className="btn" onClick={() => void api.setCoachMuted(true)}>Couper la radio</button>
        )}
      </div>
      <div className="form-row">
        <div>Bouton radio<div className="hint">{bindingLabel(s)}</div></div>
        <button className="btn" onClick={() => void learn()}>Choisir le bouton</button>
      </div>
      <div className="form-row">
        <div>Reconnaissance vocale<div className="hint">Hors ligne, limitée aux mots-clés ci-dessous{installed ? " · installée" : ""}</div></div>
        {installed ? (
          <label className="muted"><input type="checkbox" checked={s?.voice_commands ?? false} onChange={(e) => void save({ voice_commands: e.target.checked })} /> active</label>
        ) : (
          <button className="btn primary" onClick={() => void install()}>Installer</button>
        )}
      </div>
      <div className="form-row">
        <div>Annonce avant le virage<div className="hint">« Virage 16, freine plus tard » ~3 s avant le freinage du focus</div></div>
        <input type="checkbox" checked={s?.coach_callouts ?? true} onChange={(e) => void save({ coach_callouts: e.target.checked })} />
      </div>
      <div className="radio-cmds">
        {COMMANDS.map(([w, d]) => (
          <div key={w}><b>« {w} »</b> <span className="muted">{d}</span></div>
        ))}
      </div>
      {ptt && (
        <div className={`ptt-state ${ptt.state}`}>
          {ptt.state === "listening" ? "🎙 J'écoute…" : ptt.state === "error" ? `Micro : ${ptt.error}` : ptt.command ? `Compris : « ${ptt.heard} »` : `Pas compris${ptt.heard ? ` (« ${ptt.heard} »)` : ""}`}
        </div>
      )}
      {msg && <div className="muted" style={{ marginTop: 8 }}>{msg}</div>}
      <div className="muted" style={{ fontSize: 12, marginTop: 8 }}>
        Maintiens le bouton, dis le mot, relâche : la voix du coach se coupe pendant que tu parles. En qualification, la radio se tait automatiquement (sauf réponses demandées).
      </div>
    </div>
  );
}
