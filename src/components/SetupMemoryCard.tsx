import { useEffect, useState } from "react";
import { api } from "../lib/api";
import { dateTime, lapTime } from "../lib/format";
import type { EntryStatus, SetupMemory } from "../lib/types";

const n2 = (v: number) => v.toFixed(2).replace(".", ",");
const STATUS: Record<EntryStatus, string> = {
  validated: "Effet mesuré",
  not_conclusive: "Non concluant",
  not_attributable: "Non attribuable",
  no_change: "Même setup",
};

/** Résumé de la file d'essais : « Carrossage AR vers 0°, 1 cran (constat sur 2 séances sur 2) ». */
export function queueLine(q: SetupMemory["queue"][number]): string {
  return `${q.trial.label} ${q.trial.direction}, ${q.trial.steps} cran${q.trial.steps > 1 ? "s" : ""} (constat sur ${q.repeats} séance${q.repeats > 1 ? "s" : ""} sur ${q.sessions_measured})${q.tried ? ", déjà essayé sans effet" : ""}`;
}

export function MemoryView({ m }: { m: SetupMemory }) {
  return (
    <div className="sm-card">
      <h3>Setup recommandé</h3>
      {m.recommended ? <p className="sm-reco">{m.recommended.text}</p> : <p className="muted">Pas encore de séance avec setup enregistré.</p>}
      {m.recommended && m.recommended.changes.length > 0 && (
        <ul className="sm-list">
          {m.recommended.changes.map(([c, proof]) => (
            <li key={c.key}>
              <b>{c.label}</b> {c.from} → {c.to} <span className="muted">· preuve : {proof}</span>
            </li>
          ))}
        </ul>
      )}
      {m.queue.length > 0 && (
        <>
          <h3>Prochains essais (un seul à la fois)</h3>
          <ol className="sm-list">
            {m.queue.map((q) => (
              <li key={q.trial.label}>{queueLine(q)}</li>
            ))}
          </ol>
        </>
      )}
      {m.skipped.length > 0 && <p className="muted">Écartés : {m.skipped.join(" ; ")}.</p>}
      {m.entries.length > 0 && (
        <>
          <h3>Mémoire d'essais</h3>
          <table className="su-table">
            <thead>
              <tr>
                <th>Séances</th>
                <th>Changements</th>
                <th>Verdict</th>
                <th>Tour médian</th>
              </tr>
            </thead>
            <tbody>
              {m.entries.map((e) => (
                <tr key={`${e.from_session}-${e.to_session}`}>
                  <td className="muted">{dateTime(e.from_started_at)} → {dateTime(e.to_started_at)}</td>
                  <td>{e.changes.length === 0 ? "aucun" : e.changes.length > 3 ? `${e.changes.length} réglages` : e.changes.map((c) => `${c.label} ${c.from} → ${c.to}`).join(" ; ")}</td>
                  <td>
                    <b>{STATUS[e.status]}</b> <span className="muted">{e.text}</span>
                  </td>
                  <td className="num">{e.lap_before_s != null && e.lap_after_s != null ? `${lapTime(e.lap_before_s)} → ${lapTime(e.lap_after_s)}` : "—"}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </>
      )}
      {m.consumption.length > 0 && (
        <p className="muted">Consommation par setup : {m.consumption.map(([, label, per, laps, unit]) => `${label} ${n2(per)} ${unit} par tour (${laps} tours)`).join(" · ")}.</p>
      )}
      {m.missing.length > 0 && <p className="muted">{m.missing.join(" ")}</p>}
      <p className="muted sd-note">Recommandé = ton setup actuel plus les seuls changements dont l'effet a été mesuré (un réglage à la fois, au-delà de la dispersion). Jamais un optimum garanti.</p>
    </div>
  );
}

/** Mémoire d'essais de la voiture et du circuit de la séance choisie. */
export function SetupMemoryCard({ sessionId }: { sessionId: number }) {
  const [m, setM] = useState<SetupMemory | null>(null);
  useEffect(() => {
    setM(null);
    let live = true;
    api.setupMemory(sessionId).then(
      (x) => {
        if (live) setM(x);
      },
      () => {
        if (live) setM(null);
      },
    );
    return () => {
      live = false;
    };
  }, [sessionId]);
  return m ? <MemoryView m={m} /> : null;
}
