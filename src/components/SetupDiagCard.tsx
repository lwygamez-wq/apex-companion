import { useEffect, useMemo, useState } from "react";
import { api } from "../lib/api";
import { dateTime } from "../lib/format";
import { useAsync } from "../lib/hooks";
import type { BalancePhase, BalanceTrend, BalanceZone, DiagStat, SessionSummary, SetupDiagnosis, SetupRow, SetupTrialReport } from "../lib/types";
import { lapTime } from "../lib/format";
import { SetupMemoryCard } from "./SetupMemoryCard";
import { Alert, Skeleton } from "./ui";

/** « 58,7 °C » ; « — » sans mesure. */
export function fmtStat(s: DiagStat | null | undefined, digits = 1): string {
  if (!s) return "—";
  return `${s.value.toFixed(digits).replace(".", ",")}${s.unit ? ` ${s.unit}` : ""}`;
}

/** Dispersion d'un tour à l'autre, en clair : « ±0,7 sur 30 tours ». */
export function fmtSpread(s: DiagStat | null | undefined, digits = 1): string {
  if (!s) return "";
  return `±${(s.spread / 2).toFixed(digits).replace(".", ",")} sur ${s.laps} tours`;
}

const TREND: Record<BalanceTrend, { text: string; cls: string }> = {
  sous_vire: { text: "sous-vire", cls: "warn" },
  survire: { text: "survire", cls: "bad" },
  pas_d_ecart: { text: "pas d'écart", cls: "" },
};

const PHASES: BalancePhase[] = ["entree", "milieu", "sortie"];

/** Équilibre en tableau : un virage par ligne, entrée / milieu / sortie en colonnes. */
export function balanceRows(zones: BalanceZone[]): { corner: string; cells: (BalanceZone | null)[] }[] {
  const order: string[] = [];
  for (const z of zones) if (!order.includes(z.corner)) order.push(z.corner);
  return order.map((corner) => ({ corner, cells: PHASES.map((p) => zones.find((z) => z.corner === corner && z.phase === p) ?? null) }));
}

function SetupTable({ rows }: { rows: SetupRow[] }) {
  const groups = useMemo(() => {
    const m = new Map<string, SetupRow[]>();
    for (const r of rows) m.set(r.group, [...(m.get(r.group) ?? []), r]);
    return [...m.entries()];
  }, [rows]);
  return (
    <>
      {groups.map(([g, list]) => (
        <div key={g} className="su-group">
          <h3>{g}</h3>
          <table className="su-table">
            <thead>
              <tr><th>Réglage</th><th>Valeur</th><th>Cran</th><th>Crans</th></tr>
            </thead>
            <tbody>
              {list.map((r) => (
                <tr key={r.key}>
                  <td>{r.label}</td>
                  <td className="num">{r.display}</td>
                  <td className="num">{r.step}</td>
                  <td className="num muted">{r.min}–{r.max}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ))}
    </>
  );
}

export function Diagnosis({ d }: { d: SetupDiagnosis }) {
  // Constats des pneus en tête ; ceux des hauteurs sont dans leur tableau (colonne « Où »).
  const findings = d.tyre_findings;
  const balance = balanceRows(d.balance);
  return (
    <div className="sd">
      <div className="su-summary">
        <b>{d.laps_used}</b> tours utilisés · {d.track} · {d.car}
        <span className="muted"> · tours au stand, sorties, incidents et drapeaux jaunes écartés</span>
      </div>

      {findings.length > 0 && (
        <ul className="sd-findings">
          {findings.map((f) => (
            <li key={f.text}>{f.text} <span className="muted">({f.laps} tours, ±{(f.spread / 2).toFixed(1).replace(".", ",")} d'un tour à l'autre)</span></li>
          ))}
        </ul>
      )}

      {d.tyres.length > 0 && (
        <div className="su-group">
          <h3>Pneus, en appui</h3>
          <table className="su-table sd-table">
            <thead>
              <tr><th>Roue</th><th>Extérieur</th><th>Milieu</th><th>Intérieur</th><th>Carcasse</th><th>Couche interne</th><th>Échelle du jeu (estimée)</th><th>Pression à chaud</th><th>Réglée</th></tr>
            </thead>
            <tbody>
              {d.tyres.map((t) => (
                <tr key={t.wheel}>
                  <td>{t.wheel}</td>
                  <td className="num">{fmtStat(t.outer)}</td>
                  <td className="num">{fmtStat(t.middle)}</td>
                  <td className="num">{fmtStat(t.inner)}</td>
                  <td className="num">{fmtStat(t.carcass)}</td>
                  <td className="num">{fmtStat(t.inner_layer)}</td>
                  <td className="num">{fmtStat(t.game_scale)}</td>
                  <td className="num">{fmtStat(t.pressure_hot, 0)}</td>
                  <td className="num muted">{t.pressure_set ?? "—"}</td>
                </tr>
              ))}
            </tbody>
          </table>
          <p className="muted sd-note">{d.temperature_note} {d.game_scale_note} Dispersion d'un tour à l'autre : {fmtSpread(d.tyres[0]?.inner_minus_outer)} (écart intérieur / extérieur, avant gauche).</p>
        </div>
      )}

      {d.brakes.length > 0 && (
        <div className="su-group">
          <h3>Freins</h3>
          <table className="su-table sd-table">
            <thead>
              <tr><th>Roue</th><th>Médiane en piste</th><th>Pic par tour</th><th>Dispersion du pic</th></tr>
            </thead>
            <tbody>
              {d.brakes.map((b) => (
                <tr key={b.wheel}>
                  <td>{b.wheel}</td>
                  <td className="num">{fmtStat(b.median, 0)}</td>
                  <td className="num">{fmtStat(b.peak, 0)}</td>
                  <td className="num muted">{fmtSpread(b.peak, 0)}</td>
                </tr>
              ))}
            </tbody>
          </table>
          {d.locks.length > 0 ? (
            <ul className="sd-findings">
              {d.locks.map((l) => (
                <li key={`${l.corner}-${l.wheel}`}>{l.wheel} : blocage à {l.corner}, {l.laps} tours ({l.events} fois), glissement {l.slip_pct.toFixed(0)} %.</li>
              ))}
            </ul>
          ) : (
            !d.missing.some((m) => m.startsWith("Blocages")) && <p className="muted sd-note">Pas de blocage répété au même virage.</p>
          )}
        </div>
      )}

      {d.heights.length > 0 && (
        <div className="su-group">
          <h3>Hauteur de caisse</h3>
          <table className="su-table sd-table">
            <thead>
              <tr><th>Roue</th><th>Médiane</th><th>Point bas</th><th>Où</th><th>Pointes &lt; 5 mm</th></tr>
            </thead>
            <tbody>
              {d.heights.map((h) => (
                <tr key={h.wheel}>
                  <td>{h.wheel}</td>
                  <td className="num">{fmtStat(h.median)}</td>
                  <td className="num">{fmtStat(h.low)}</td>
                  <td>{h.low_zone ?? <span className="muted">réparti</span>}</td>
                  <td className="num muted">{h.spikes_under_5mm}</td>
                </tr>
              ))}
            </tbody>
          </table>
          <p className="muted sd-note">Point bas : 1 % le plus bas de chaque tour. Le jeu ne dit pas quand le fond touche : Apex montre où la voiture est la plus basse, sans parler de talonnage. Les pointes isolées sous 5 mm sont comptées, pas interprétées.</p>
        </div>
      )}

      {balance.length > 0 && (
        <div className="su-group">
          <h3>Équilibre par virage</h3>
          <table className="su-table sd-table">
            <thead>
              <tr><th>Virage</th><th>Entrée</th><th>Milieu</th><th>Sortie</th></tr>
            </thead>
            <tbody>
              {balance.map((r) => (
                <tr key={r.corner}>
                  <td>{r.corner}</td>
                  {r.cells.map((z, i) => (
                    <td key={i} className="num">
                      {z ? (
                        <>
                          {TREND[z.trend].cls ? <span className={`chip ${TREND[z.trend].cls}`}>{TREND[z.trend].text}</span> : <span className="muted">{TREND[z.trend].text}</span>}{" "}
                          <span className="mono muted">{z.ratio.value.toFixed(2).replace(".", ",")}</span>
                        </>
                      ) : (
                        <span className="muted">—</span>
                      )}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
          <p className="muted sd-note">Rotation de la voiture pour un même angle de volant, comparée aux courbes prises en douceur à la même vitesse (1,00 = pareil). Sous 0,85 sur 8 tours sur 10 : sous-vire ; au-dessus de 1,15 : survire. Le sens est fiable, l'ampleur ne se compare qu'entre virages de cette séance.</p>
        </div>
      )}

      {d.setup.length > 0 && (
        <details className="sd-setup">
          <summary>Setup de la séance ({d.setup.length} réglages, valeurs du jeu)</summary>
          <SetupTable rows={d.setup} />
        </details>
      )}

      {d.missing.map((m) => (
        <p key={m} className="muted rd-missing">Non mesuré. {m}</p>
      ))}
    </div>
  );
}

/** Essai guidé : un seul réglage à essayer, puis le verdict mesuré sur la séance d'après. */
export function TrialView({ r, afterChoices, after, onAfter }: { r: SetupTrialReport; afterChoices: SessionSummary[]; after: number | null; onAfter: (id: number | null) => void }) {
  return (
    <div className="su-group sd-trial">
      <h3>Essai guidé</h3>
      {!r.proposal && <p className="muted sd-note">{r.no_proposal}</p>}
      {r.proposal && (
        <>
          <p className="sd-trial-what"><b>{r.proposal.what}</b></p>
          <p className="muted sd-note">Pourquoi : {r.proposal.why}</p>
          <p className="muted sd-note">Comment : {r.proposal.protocol} Aucun gain de temps n'est promis : on mesure l'effet.</p>
          <label className="su-pick mt-12">
            <span>Séance d'après</span>
            <select value={after ?? ""} onChange={(e) => onAfter(e.target.value ? Number(e.target.value) : null)}>
              <option value="">Choisir la séance avec le nouveau réglage…</option>
              {afterChoices.map((s) => (
                <option key={s.id} value={s.id}>{dateTime(s.started_at_unix)} · {s.lap_count} tours</option>
              ))}
            </select>
          </label>
          {r.verdict && (
            <div className="sd-verdict">
              <p className={r.verdict.applied ? "" : "fc-warn"}>{r.verdict.applied_text}</p>
              <p><span className={`chip ${r.verdict.significant && r.verdict.applied ? "good" : ""}`}>{r.verdict.significant ? "effet mesuré" : "pas d'effet mesurable"}</span> {r.verdict.text}</p>
              {r.verdict.others_changed.length > 0 && <p className="muted sd-note">Autres réglages changés : {r.verdict.others_changed.join(" ; ")}.</p>}
              {r.verdict.lap_before_s != null && r.verdict.lap_after_s != null && (
                <p className="muted sd-note">Temps au tour médian, à titre indicatif (piste, carburant et rythme changent aussi) : {lapTime(r.verdict.lap_before_s)} puis {lapTime(r.verdict.lap_after_s)}.</p>
              )}
            </div>
          )}
        </>
      )}
    </div>
  );
}

/** Onglet Setups : diagnostic d'une séance LMU, mesures seulement. */
export function SetupDiagCard() {
  const sessions = useAsync(() => api.sessions({ sim: "lmu" }), []);
  const list = (sessions.data ?? []).filter((s) => s.external_driver == null);
  const [id, setId] = useState<number | null>(null);
  const [d, setD] = useState<SetupDiagnosis | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const chosen = id ?? list[0]?.id ?? null;
  const cur = list.find((s) => s.id === chosen);
  // Séance d'après : même circuit, même voiture, plus récente.
  const afterChoices = cur ? list.filter((s) => s.track === cur.track && s.car === cur.car && s.track_config === cur.track_config && s.started_at_unix > cur.started_at_unix) : [];
  const [after, setAfter] = useState<number | null>(null);
  const [trial, setTrial] = useState<SetupTrialReport | null>(null);
  useEffect(() => {
    setTrial(null);
    if (chosen == null) return;
    let live = true;
    api.setupTrial(chosen, after).then(
      (x) => {
        if (live) setTrial(x);
      },
      () => {
        if (live) setTrial(null);
      },
    );
    return () => {
      live = false;
    };
  }, [chosen, after]);

  useEffect(() => {
    setD(null);
    setErr(null);
    if (chosen == null) return;
    let live = true;
    setBusy(true);
    api.setupDiagnosis(chosen).then(
      (x) => {
        if (live) setD(x);
      },
      (e: unknown) => {
        if (live) setErr(e instanceof Error ? e.message : String(e));
      },
    ).finally(() => {
      if (live) setBusy(false);
    });
    return () => {
      live = false;
    };
  }, [chosen]);

  return (
    <div className="card">
      <h2>Diagnostic de séance</h2>
      <p className="muted sd-note m-0">Ce que la voiture a fait, mesuré sur tes tours. Aucun conseil de réglage ici.</p>
      {list.length === 0 ? (
        <div className="muted">Aucune séance Le Mans Ultimate enregistrée.</div>
      ) : (
        <label className="su-pick">
          <span>Séance</span>
          <select value={chosen ?? ""} onChange={(e) => { setId(Number(e.target.value)); setAfter(null); }}>
            {list.map((s) => (
              <option key={s.id} value={s.id}>{dateTime(s.started_at_unix)} · {s.track} · {s.car} · {s.lap_count} tours</option>
            ))}
          </select>
        </label>
      )}
      {busy && <Skeleton lines={4} label="Lecture de l'enregistrement" />}
      {err && <Alert tone="danger" title="Diagnostic impossible">{err}</Alert>}
      {d && <Diagnosis d={d} />}
      {d && trial && <TrialView r={trial} afterChoices={afterChoices} after={after} onAfter={setAfter} />}
      {d && chosen != null && <SetupMemoryCard sessionId={chosen} />}
    </div>
  );
}
