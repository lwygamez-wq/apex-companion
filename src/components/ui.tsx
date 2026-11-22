// Composants de base du système de design (docs/audit-visuel.md). Styles : section « Composants de base »
// de styles.css. Chaque composant a ses états : survol, focus visible, désactivé, et pour les blocs de
// données : chargement (Skeleton), vide (EmptyState), erreur (Alert avec « Réessayer »).
import type { ReactNode } from "react";

type Variant = "primary" | "secondary" | "quiet";

/** Bouton : primaire (une seule action principale par page), secondaire (contour) ou discret (texte). */
export function Button({ variant = "secondary", children, ...rest }: { variant?: Variant; children: ReactNode } & React.ButtonHTMLAttributes<HTMLButtonElement>) {
  return (
    <button type="button" {...rest} className={`ui-btn ui-btn-${variant}${rest.className ? ` ${rest.className}` : ""}`}>
      {children}
    </button>
  );
}

/** Carte : un titre (ce que la carte permet de décider), une action facultative, le contenu. */
export function Card({ title, action, children, className }: { title?: ReactNode; action?: ReactNode; children: ReactNode; className?: string }) {
  return (
    <section className={`card${className ? ` ${className}` : ""}`}>
      {(title || action) && (
        <header className="ui-card-head">
          {title && <h2>{title}</h2>}
          {action}
        </header>
      )}
      {children}
    </section>
  );
}

export type Tone = "neutral" | "best" | "gain" | "loss" | "info" | "danger";

/** Écart signé : négatif = gain (vert), positif = perte (jaune). */
export function deltaTone(v: number | null | undefined): Tone {
  if (v == null || Math.abs(v) < 1e-9) return "neutral";
  return v < 0 ? "gain" : "loss";
}

/** Chiffre clé : libellé, valeur, unité, écart facultatif, ligne d'explication. */
export function Stat({ label, value, unit, delta, tone = "neutral", hint, title }: { label: ReactNode; value: ReactNode; unit?: string; delta?: { text: string; tone: Tone }; tone?: Tone; hint?: ReactNode; title?: string }) {
  return (
    <div className="ui-stat" title={title}>
      <span className="ui-stat-label">{label}</span>
      <span className={`ui-stat-value tone-${tone}`}>
        {value}
        {unit && <small>{unit}</small>}
      </span>
      {delta && <span className={`ui-stat-delta tone-${delta.tone}`}>{delta.text}</span>}
      {hint && <span className="ui-stat-hint">{hint}</span>}
    </div>
  );
}

/** Badge d'état : une couleur seulement quand elle a un sens (meilleur, gain, perte, info, erreur). */
export function Badge({ tone = "neutral", children, title }: { tone?: Tone; children: ReactNode; title?: string }) {
  return (
    <span className={`ui-badge tone-${tone}`} title={title}>
      {children}
    </span>
  );
}

/** Alerte : information, avertissement ou erreur, avec une action (souvent « Réessayer »). */
export function Alert({ tone = "info", title, children, action }: { tone?: "info" | "warn" | "danger"; title?: ReactNode; children?: ReactNode; action?: ReactNode }) {
  return (
    <div className={`ui-alert ui-alert-${tone}`} role={tone === "danger" ? "alert" : "status"}>
      <div className="ui-alert-body">
        {title && <b>{title}</b>}
        {children && <div>{children}</div>}
      </div>
      {action}
    </div>
  );
}

/** Erreur de chargement, avec « Réessayer ». */
export function LoadError({ error, onRetry }: { error: string; onRetry?: () => void }) {
  return (
    <Alert tone="danger" title="Lecture impossible" action={onRetry && <Button onClick={onRetry}>Réessayer</Button>}>
      {error}
    </Alert>
  );
}

/** État vide : ce qui manque et quoi faire. */
export function EmptyState({ title, children, action }: { title: ReactNode; children?: ReactNode; action?: ReactNode }) {
  return (
    <div className="ui-empty">
      <h2>{title}</h2>
      {children && <p>{children}</p>}
      {action && <div className="ui-empty-action">{action}</div>}
    </div>
  );
}

/** Chargement : blocs gris à la place du contenu attendu (pas de page vide). */
export function Skeleton({ lines = 3, block = false, label = "Chargement…" }: { lines?: number; block?: boolean; label?: string }) {
  return (
    <div className="ui-skeleton" role="status" aria-label={label}>
      {block && <div className="ui-skel ui-skel-block" />}
      {Array.from({ length: lines }, (_, i) => (
        <div key={i} className="ui-skel" style={{ width: `${92 - i * 14}%` }} />
      ))}
    </div>
  );
}

/** Onglets / bascule à choix unique. */
export function Tabs<T extends string>({ value, options, onChange, label }: { value: T; options: { value: T; label: string; title?: string }[]; onChange: (v: T) => void; label: string }) {
  return (
    <div className="ui-tabs" role="tablist" aria-label={label}>
      {options.map((o) => (
        <button key={o.value} type="button" role="tab" aria-selected={o.value === value} className={o.value === value ? "on" : ""} title={o.title} onClick={() => onChange(o.value)}>
          {o.label}
        </button>
      ))}
    </div>
  );
}

/** Info-bulle accessible : le texte apparaît au survol et au focus clavier. */
export function Tooltip({ text, children }: { text: string; children: ReactNode }) {
  return (
    <span className="ui-tip" tabIndex={0} aria-label={text}>
      {children}
      <span className="ui-tip-text" role="tooltip">
        {text}
      </span>
    </span>
  );
}
