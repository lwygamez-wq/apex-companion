import { api } from "../lib/api";
import { useAsync } from "../lib/hooks";
import type { StatusPayload } from "../lib/types";

type Item = { ok: boolean | null; label: string; detail: string };

/**
 * « Prêt à rouler » : ce qui doit marcher avant de partir en piste (simulateur, coach vocal,
 * bouton radio, LEDs), en une ligne chacun. Vert = prêt, jaune = à régler, gris = facultatif.
 */
export function ReadyCard({ status, onSetup }: { status: StatusPayload | null; onSetup: () => void }) {
  const settings = useAsync(() => api.settings(), []);
  const s = settings.data;
  const st = status?.state;
  const items: Item[] = [
    st?.state === "session_active"
      ? { ok: true, label: "Simulateur", detail: "en piste, Apex suit ta voiture" }
      : st && st.state !== "no_simulator"
        ? { ok: true, label: "Simulateur", detail: status!.label }
        : { ok: null, label: "Simulateur", detail: "lance iRacing ou Le Mans Ultimate" },
    s
      ? s.live_enabled
        ? { ok: s.coach_audio, label: "Coach vocal", detail: s.coach_audio ? "actif, il te lit ce programme au départ" : "coupé : réactive-le dans Au volant" }
        : { ok: false, label: "Coach vocal", detail: "télémétrie en direct désactivée dans les Réglages" }
      : { ok: null, label: "Coach vocal", detail: "…" },
    { ok: s?.ptt_binding ? true : null, label: "Bouton radio", detail: s?.ptt_binding ? s.ptt_binding.name : "facultatif : un bouton du volant pour parler au coach" },
    { ok: s?.led_cues ? true : null, label: "LEDs du volant", detail: s?.led_cues ? "repères de freinage envoyés à SimHub" : "facultatif : repères de freinage sur les LEDs" },
  ];
  const ready = items[1]!.ok === true;
  return (
    <div className="card ready">
      <div className="row" style={{ justifyContent: "space-between" }}>
        <h2 style={{ margin: 0 }}>{ready ? "Prêt à rouler" : "Avant de rouler"}</h2>
        <button className="btn ghost small" onClick={onSetup}>Régler le coach</button>
      </div>
      <ul className="ready-list">
        {items.map((i) => (
          <li key={i.label}>
            <span className={`dot ${i.ok === true ? "on" : i.ok === false ? "wait" : ""}`} />
            <b>{i.label}</b>
            <span className="muted">{i.detail}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}
