import { useEffect, useRef, useState } from "react";
import { api } from "../lib/api";
import type { ChatMessage } from "../lib/types";

const STARTERS = [
  "Résume ma séance en 3 points.",
  "Où est-ce que je perds le plus de temps, et pourquoi ?",
  "Qu'est-ce que je travaille à la prochaine séance ?",
  "Explique-moi mes sorties de piste.",
];

/**
 * Débrief par IA : conversation sur la séance affichée. L'IA ne reçoit que la synthèse chiffrée
 * calculée par Apex (bouton « Voir ce qui est envoyé »), jamais la télémétrie brute.
 */
export function DebriefChat({ sessionId, configured, onSetup, question, onAsked }: { sessionId: number; configured: boolean; onSetup: () => void; question?: string | null; onAsked?: () => void }) {
  const [msgs, setMsgs] = useState<ChatMessage[]>([]);
  const [draft, setDraft] = useState("");
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [cost, setCost] = useState(0);
  const [ctx, setCtx] = useState<string | null>(null);
  const end = useRef<HTMLDivElement>(null);

  // Nouvelle séance : nouvelle conversation.
  useEffect(() => {
    setMsgs([]);
    setErr(null);
    setCost(0);
    setCtx(null);
  }, [sessionId]);
  useEffect(() => {
    end.current?.scrollIntoView({ block: "end" });
  }, [msgs, busy]);

  const ask = async (q: string) => {
    const text = q.trim();
    if (!text || busy) return;
    const next: ChatMessage[] = [...msgs, { role: "user", text }];
    setMsgs(next);
    setDraft("");
    setBusy(true);
    setErr(null);
    try {
      const r = await api.aiChat(sessionId, next);
      setMsgs([...next, { role: "assistant", text: r.text }]);
      setCost((c) => c + r.cost_usd);
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
      setMsgs(msgs);
      setDraft(text);
    } finally {
      setBusy(false);
    }
  };

  // Question venue d'un conseil (« Demander à l'ingé ») : posée une fois, à l'ouverture.
  useEffect(() => {
    if (question && configured && !busy) {
      onAsked?.();
      void ask(question);
    } else if (question && !configured) {
      setDraft(question);
      onAsked?.();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [question, configured]);

  if (!configured) {
    return (
      <div className="card empty">
        <h2>Débrief IA</h2>
        <p>Pose tes questions sur la séance à un ingénieur de piste IA : il répond uniquement avec les chiffres mesurés par Apex.</p>
        <p className="muted">Il faut une clé API Claude (compte sur console.anthropic.com, facturé à l'usage : quelques centimes par question).</p>
        <button className="btn primary" onClick={onSetup}>Ajouter ma clé dans les Réglages</button>
      </div>
    );
  }

  return (
    <div className="card debrief">
      <div className="row" style={{ justifyContent: "space-between", alignItems: "baseline" }}>
        <h2 style={{ margin: 0 }}>Débrief IA</h2>
        <span className="muted" style={{ fontSize: 13 }}>
          {cost > 0 && `≈ ${cost < 0.01 ? "< 0,01" : cost.toFixed(2).replace(".", ",")} $ · `}
          <button className="link" onClick={() => void (ctx ? setCtx(null) : api.aiContext(sessionId).then(setCtx, (e: unknown) => setErr(String(e))))}>
            {ctx ? "masquer les données" : "voir ce qui est envoyé"}
          </button>
        </span>
      </div>
      {ctx && <pre className="debrief-ctx">{ctx}</pre>}
      <div className="debrief-log">
        {msgs.length === 0 && (
          <div className="debrief-starters">
            {STARTERS.map((s) => (
              <button key={s} className="btn ghost small" onClick={() => void ask(s)} disabled={busy}>{s}</button>
            ))}
          </div>
        )}
        {msgs.map((m, i) => (
          <div key={i} className={`debrief-msg ${m.role}`}>{m.text}</div>
        ))}
        {busy && <div className="debrief-msg assistant muted">L'ingénieur regarde tes données…</div>}
        <div ref={end} />
      </div>
      {err && <div className="error" style={{ marginTop: 8 }}>{err}</div>}
      <form
        className="debrief-input"
        onSubmit={(e) => {
          e.preventDefault();
          void ask(draft);
        }}
      >
        <input value={draft} onChange={(e) => setDraft(e.target.value)} placeholder="Ta question sur la séance…" disabled={busy} aria-label="Question" />
        <button className="btn primary" disabled={busy || !draft.trim()}>Envoyer</button>
      </form>
    </div>
  );
}
