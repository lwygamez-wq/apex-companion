import { open } from "@tauri-apps/plugin-dialog";
import { useEffect, useMemo, useState } from "react";
import { api } from "../lib/api";
import { useAsync } from "../lib/hooks";
import type { ProbeReport, SetupComparison, SetupEntry } from "../lib/types";

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

/** Verdict de la sonde pour les conditions du Setup Builder. */
function ProbeCard({ reports }: { reports: ProbeReport[] }) {
  const r = reports[0];
  if (!r) {
    return (
      <div className="card su-probe">
        <h2>Sonde setup (lecture seule)</h2>
        <p className="muted" style={{ margin: 0 }}>Pas encore de mesure. Roule 5 tours en LMU avec Apex ouvert : à la fin de la séance, Apex note ce que le jeu fournit (réglages, crans mini et maxi, setups du jeu, températures de pneus). Rien n'est écrit dans le jeu.</p>
      </div>
    );
  }
  const f = (n: string) => r.fields.find((x) => x.name === n);
  const pct = (n: string) => f(n)?.filled_pct ?? 0;
  const apiOk = r.api.some((a) => a.ok);
  const rows: { label: string; ok: boolean | null; detail: string }[] = [
    {
      label: "Crans mini et maxi des réglages",
      ok: apiOk ? r.garage.with_range > 0 : null,
      detail: apiOk ? `${r.garage.with_range} réglages avec plage sur ${r.garage.settings}` : "API du jeu injoignable pendant la séance",
    },
    {
      label: "Setups fournis par le jeu",
      ok: apiOk ? (r.garage.presets ?? 0) > 0 : null,
      detail: apiOk ? (r.garage.presets == null ? "champ absent" : `${r.garage.presets} setup(s)`) : "API du jeu injoignable",
    },
    {
      label: "Température idéale des pneus",
      ok: pct("pneu_temperature_optimale") > 0 || r.garage.optimal_conditions === true,
      detail: pct("pneu_temperature_optimale") > 0 ? `fournie (${Math.round(f("pneu_temperature_optimale")!.mean)})` : r.garage.optimal_conditions ? "fournie par l'API" : "non fournie",
    },
    { label: "Températures pneus (3 zones)", ok: pct("pneu_surface_c") > 50, detail: `${pct("pneu_surface_c")} % des mesures` },
    { label: "Hauteurs de caisse", ok: pct("caisse_hauteur_av_m") > 50, detail: `${pct("caisse_hauteur_av_m")} % des mesures` },
    { label: "Appui aéro AV / AR", ok: pct("appui_av_n") > 50, detail: `${pct("appui_av_n")} % des mesures` },
  ];
  return (
    <div className="card su-probe">
      <h2>Sonde setup (lecture seule)</h2>
      <div className="muted" style={{ marginBottom: 8 }}>
        Dernière séance : {r.track} · {r.car} · {r.frames} mesures en piste
      </div>
      <table className="su-table">
        <tbody>
          {rows.map((x) => (
            <tr key={x.label}>
              <td className={`su-flag ${x.ok == null ? "unk" : x.ok ? "ok" : "ko"}`}>{x.ok == null ? "?" : x.ok ? "OUI" : "NON"}</td>
              <td>{x.label}</td>
              <td className="muted">{x.detail}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

/** Onglet Setups : comparateur de setups LMU et sonde du Setup Builder. */
export function SetupsPage() {
  const [dir, setDir] = useState<string | null>(() => load(DIR_KEY));
  const lib = useAsync(() => api.setupLibrary(dir), [dir ?? ""]);
  const probe = useAsync(() => api.setupProbeReports().catch(() => [] as ProbeReport[]), []);
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
      <ProbeCard reports={probe.data ?? []} />
    </div>
  );
}
