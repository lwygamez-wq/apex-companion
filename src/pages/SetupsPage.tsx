import { open } from "@tauri-apps/plugin-dialog";
import { useEffect, useMemo, useState } from "react";
import { api } from "../lib/api";
import { useAsync } from "../lib/hooks";
import type { SetupComparison, SetupEntry } from "../lib/types";

const DIR_KEY = "apex.setups.dir";
const load = (k: string) => {
  try {
    return localStorage.getItem(k);
  } catch {
    return null;
  }
};
const store = (k: string, v: string | null) => {
  try {
    if (v == null) localStorage.removeItem(k);
    else localStorage.setItem(k, v);
  } catch {
    /* stockage indisponible : choix non retenu */
  }
};

/** « Hypercar Peugeot_9x8 WEC2026 » → « Peugeot 9x8 (Hypercar, WEC2026) ». */
export function carLabel(vehicleClass: string): string {
  const parts = vehicleClass.split(/\s+/).filter(Boolean);
  const model = parts.find((p) => p.includes("_"));
  if (!model) return vehicleClass || "Voiture inconnue";
  const rest = parts.filter((p) => p !== model).join(", ");
  return `${model.replace(/_/g, " ")}${rest ? ` (${rest})` : ""}`;
}

const stepTxt = (n: number) => (n === 0 ? "=" : `${n > 0 ? "+" : "−"}${Math.abs(n)} cran${Math.abs(n) > 1 ? "s" : ""}`);

function SetupPicker({ label, files, value, onChange }: { label: string; files: SetupEntry[]; value: string; onChange: (p: string) => void }) {
  const groups = useMemo(() => {
    const m = new Map<string, SetupEntry[]>();
    for (const f of files) {
      const k = `${f.track} · ${carLabel(f.vehicle_class)}`;
      m.set(k, [...(m.get(k) ?? []), f]);
    }
    return [...m.entries()];
  }, [files]);
  return (
    <label className="su-pick">
      <span>{label}</span>
      <select value={value} onChange={(e) => onChange(e.target.value)}>
        <option value="">Choisir un setup…</option>
        {groups.map(([g, list]) => (
          <optgroup key={g} label={g}>
            {list.map((f) => (
              <option key={f.path} value={f.path}>{f.name}</option>
            ))}
          </optgroup>
        ))}
      </select>
    </label>
  );
}

function ComparisonTable({ c }: { c: SetupComparison }) {
  const groups = useMemo(() => {
    const m = new Map<string, SetupComparison["changes"]>();
    for (const ch of c.changes) m.set(ch.group, [...(m.get(ch.group) ?? []), ch]);
    return [...m.entries()];
  }, [c]);
  return (
    <>
      <div className="su-summary">
        <b>{c.changes.length}</b> réglage{c.changes.length > 1 ? "s" : ""} différent{c.changes.length > 1 ? "s" : ""} · {c.unchanged} identiques
      </div>
      {!c.same_car && <div className="fc-warn">Deux voitures différentes ({carLabel(c.vehicle_a)} / {carLabel(c.vehicle_b)}) : les crans ne se comparent pas.</div>}
      {c.changes.length === 0 && <div className="muted">Les deux setups sont identiques.</div>}
      {groups.map(([g, list]) => (
        <div key={g} className="su-group">
          <h3>{g}</h3>
          <table className="su-table">
            <thead>
              <tr>
                <th>Réglage</th>
                <th>A</th>
                <th>B</th>
                <th>Écart</th>
                <th>Effet attendu (sens, pas un gain)</th>
              </tr>
            </thead>
            <tbody>
              {list.map((ch) => (
                <tr key={ch.label}>
                  <td>{ch.label}</td>
                  <td className="num">{ch.from || ch.from_index}</td>
                  <td className="num">{ch.to || ch.to_index}</td>
                  <td className="num">{stepTxt(ch.steps)}</td>
                  <td className="muted">{ch.meaning ?? "—"}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ))}
    </>
  );
}

/** Onglet Setups : comparateur de setups LMU. */
export function SetupsPage() {
  const [dir, setDir] = useState<string | null>(() => load(DIR_KEY));
  const lib = useAsync(() => api.setupLibrary(dir), [dir ?? ""]);
  const files = lib.data?.files ?? [];
  const [a, setA] = useState("");
  const [b, setB] = useState("");
  const [cmp, setCmp] = useState<SetupComparison | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const carA = files.find((f) => f.path === a)?.vehicle_class;
  // B : même voiture que A par défaut (les crans d'une autre voiture ne se comparent pas).
  const filesB = carA ? files.filter((f) => f.vehicle_class === carA && f.path !== a) : files;

  useEffect(() => {
    setCmp(null);
    setErr(null);
    if (!a || !b) return;
    let live = true;
    api.setupCompare(a, b).then(
      (c) => {
        if (live) setCmp(c);
      },
      (e: unknown) => {
        if (live) setErr(e instanceof Error ? e.message : String(e));
      },
    );
    return () => {
      live = false;
    };
  }, [a, b]);

  const choose = () => {
    void open({ directory: true }).then(
      (d) => {
        if (typeof d === "string") {
          store(DIR_KEY, d);
          setDir(d);
          setA("");
          setB("");
        }
      },
      (e: unknown) => setErr(String(e)),
    );
  };

  return (
    <div className="page setups-page">
      <div className="page-title">
        <h1>Setups</h1>
      </div>
      <div className="card">
        <h2>Comparer deux setups</h2>
        <div className="su-dir">
          <span className="muted">{lib.data?.dir ? `Dossier : ${lib.data.dir}` : "Dossier des setups LMU introuvable dans Steam."}</span>
          <button className="btn ghost" onClick={choose}>Choisir le dossier…</button>
          {dir && (
            <button className="btn ghost" onClick={() => { store(DIR_KEY, null); setDir(null); }}>Dossier Steam</button>
          )}
        </div>
        {lib.data?.dir && files.length === 0 && <div className="muted">Aucun setup (.svm) dans ce dossier. Le bon dossier est « Le Mans Ultimate\UserData\player\Settings ».</div>}
        {files.length > 0 && (
          <div className="su-picks">
            <SetupPicker label="Setup A" files={files} value={a} onChange={(p) => { setA(p); setB(""); }} />
            <SetupPicker label="Setup B" files={filesB} value={b} onChange={setB} />
          </div>
        )}
        {err && <div className="fc-warn">{err}</div>}
        {cmp && <ComparisonTable c={cmp} />}
        <p className="muted su-note">Apex lit les fichiers sans les modifier. Les valeurs viennent du jeu ; l'effet indiqué est le sens habituel, son ampleur dépend de la voiture et se vérifie en piste.</p>
      </div>
    </div>
  );
}
