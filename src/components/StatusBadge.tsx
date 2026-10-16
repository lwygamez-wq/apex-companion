import type { StatusPayload } from "../lib/types";

const SIM_FR: Record<string, string> = {
  iracing: "iRacing",
  lmu: "Le Mans Ultimate",
  assetto_corsa: "Assetto Corsa",
  ac_evo: "AC EVO",
  acc: "ACC",
  demo: "Démo",
};

export function StatusBadge({ status }: { status: StatusPayload | null }) {
  if (!status) return <div className="status"><span className="dot" /> Surveillance inactive</div>;
  const st = status.state;
  const cls = st.state === "session_active" ? "on" : st.state === "no_simulator" ? "" : "wait";
  const sim = "sim" in st ? SIM_FR[st.sim] ?? st.sim : null;
  return (
    <div className="status" title={status.last_auto_import ?? undefined}>
      <span className={`dot ${cls}`} />
      <div>
        <div>{status.label}</div>
        {sim && <div className="muted">{sim}</div>}
        {status.importing && <div className="muted">{status.importing}</div>}
        {st.state === "session_active" && status.live?.disk_logging != null && (
          <div className={status.live.disk_logging ? "neg" : "pos"}>
            {status.live.disk_logging ? "● Télémétrie enregistrée" : "● Télémétrie NON enregistrée"}
          </div>
        )}
      </div>
    </div>
  );
}
