import { open } from "@tauri-apps/plugin-dialog";
import { useState } from "react";
import { api } from "../lib/api";
import type { ImportReport } from "../lib/types";

function summarize(r: ImportReport[]): string {
  if (r.length === 0) return "Aucun fichier trouvé.";
  const refs = r.filter((x) => x.outcome?.status === "inserted" && /\.csv$/i.test(x.path)).length;
  const ins = r.filter((x) => x.outcome?.status === "inserted").length - refs;
  const dup = r.filter((x) => x.outcome?.status === "duplicate").length;
  const err = r.filter((x) => x.error);
  let s = `${ins} session(s) importée(s)`;
  if (refs) s += `, ${refs} tour(s) de référence Garage 61 importé(s) : ils servent maintenant de référence sur ce circuit`;
  if (dup) s += `, ${dup} déjà présente(s)`;
  if (err.length) s += `, ${err.length} erreur(s) : ${err.map((e) => e.error).join(" ; ")}`;
  return s;
}

export function ImportActions({ onDone }: { onDone: () => void }) {
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<string | null>(null);

  const run = async (fn: () => Promise<string>) => {
    setBusy(true);
    setMsg(null);
    try {
      setMsg(await fn());
      onDone();
    } catch (e) {
      setMsg(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div>
      <div className="row">
        <button className="btn primary" disabled={busy} onClick={() => run(async () => {
          const sel = await open({ multiple: true, filters: [{ name: "Télémétrie iRacing (.ibt) ou tour Garage 61 (.csv)", extensions: ["ibt", "csv"] }] });
          if (!sel) return "Import annulé.";
          return summarize(await api.importFiles(Array.isArray(sel) ? sel : [sel]));
        })} title="Fichiers .ibt d'iRacing, ou tour d'un autre pilote exporté de Garage 61 (.csv) pour t'en servir de référence">Importer (.ibt, Garage 61)</button>
        <button className="btn" disabled={busy} onClick={() => run(async () => summarize(await api.scanIracing()))}>Scanner le dossier iRacing</button>
        <button className="btn" disabled={busy} onClick={() => run(async () => {
          const r = await api.loadDemo();
          return r.status === "inserted" ? "Session de démonstration ajoutée." : "Session de démonstration déjà présente.";
        })}>Charger une session démo</button>
      </div>
      {msg && <div className="muted" style={{ marginTop: 8 }}>{msg}</div>}
    </div>
  );
}
