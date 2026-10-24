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
  if (refs) s += `, ${refs} tour(s) de référence Garage 61 importé(s) : précise s'il s'agit d'une série Fixed ou Open dans « Références d'autres pilotes » ci-dessous, pour qu'il ne serve qu'aux sessions du même type`;
  if (dup) s += `, ${dup} déjà présente(s)`;
  if (err.length) s += `, ${err.length} erreur(s) : ${err.map((e) => e.error).join(" ; ")}`;
  return s;
}

/** Boutons d'import. `compact` : sans la démo (déjà des sessions), pour une barre d'outils. */
export function ImportActions({ onDone, compact = false }: { onDone: () => void; compact?: boolean }) {
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
        })} title="Fichiers .ibt d'iRacing, ou tour d'un autre pilote exporté de Garage 61 (.csv) pour t'en servir de référence">{compact ? "Importer" : "Importer (.ibt, Garage 61)"}</button>
        <button className={compact ? "btn ghost" : "btn"} disabled={busy} onClick={() => run(async () => summarize(await api.scanIracing()))} title="Importe les .ibt du dossier iRacing pas encore importés">{compact ? "Scanner iRacing" : "Scanner le dossier iRacing"}</button>
        {!compact && (
          <button className="btn" disabled={busy} onClick={() => run(async () => {
            const r = await api.loadDemo();
            return r.status === "inserted" ? "Session de démonstration ajoutée." : "Session de démonstration déjà présente.";
          })}>Charger une session démo</button>
        )}
      </div>
      {msg && <div className="muted" style={{ marginTop: 8 }}>{msg}</div>}
    </div>
  );
}
