// Couleurs des jetons (styles.css) pour ce qui ne lit pas les variables CSS : canvas des graphiques uPlot.
// Le SVG, lui, prend directement `var(--jeton)`.

const cache = new Map<string, string>();

/** Valeur d'un jeton de couleur (`--loss` → « #f5c542 »). Repli neutre hors navigateur (tests). */
export function tok(name: string): string {
  const hit = cache.get(name);
  if (hit) return hit;
  const v = typeof document === "undefined" ? "" : getComputedStyle(document.documentElement).getPropertyValue(`--${name}`).trim();
  const out = v || "gray";
  if (v) cache.set(name, out);
  return out;
}

/** Jeton hexadécimal avec transparence, pour un canvas : `alpha("gain", 0.18)` → « rgba(61, 217, 179, 0.18) ». */
export function alpha(name: string, a: number): string {
  return hexAlpha(tok(name), a);
}

export function hexAlpha(hex: string, a: number): string {
  const m = /^#([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i.exec(hex);
  if (!m) return hex;
  return `rgba(${parseInt(m[1]!, 16)}, ${parseInt(m[2]!, 16)}, ${parseInt(m[3]!, 16)}, ${a})`;
}

/** Même chose pour un attribut SVG : `color-mix` sur la variable CSS. */
export const mix = (name: string, a: number) => `color-mix(in srgb, var(--${name}) ${Math.round(a * 100)}%, transparent)`;
