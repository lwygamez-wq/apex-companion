// Garde-fous du système de design (docs/audit-visuel.md) : vérifiés à chaque CI.
import { readFileSync, readdirSync, statSync } from "node:fs";
import { describe, expect, it } from "vitest";

declare const process: { cwd(): string };
const join = (...p: string[]) => p.join("/");
// Vitest tourne à la racine du dépôt.
const here = join(process.cwd(), "src");

const css = readFileSync(join(here, "styles.css"), "utf8");
const rootEnd = css.indexOf("}", css.indexOf(":root {"));
const root = css.slice(css.indexOf(":root {"), rootEnd);
// Règles du desktop et de l'app iPhone (même système de design).
const rules = css.slice(rootEnd + 1) + readFileSync(join(here, "mobile/mobile.css"), "utf8");
const token = (name: string): string => {
  const m = new RegExp(`--${name}:\\s*(#[0-9a-fA-F]{6})`).exec(root);
  if (!m) throw new Error(`jeton --${name} absent`);
  return m[1]!;
};
const lum = (hex: string) => {
  const c = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255).map((v) => (v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4));
  return 0.2126 * c[0]! + 0.7152 * c[1]! + 0.0722 * c[2]!;
};
const contrast = (a: string, b: string) => {
  const [x, y] = [lum(a), lum(b)].sort((p, q) => q - p);
  return (x! + 0.05) / (y! + 0.05);
};
const surfaces = ["bg-sunken", "bg", "surface-1", "surface-2", "surface-3"];

describe("système de design", () => {
  it("texte : contraste ≥ 4,5:1 sur toutes les surfaces", () => {
    for (const t of ["text", "text-2", "best", "best-text", "gain", "loss", "danger", "danger-text", "info", "accent", "ref"]) {
      for (const s of surfaces) expect(contrast(token(t), token(s)), `${t} sur ${s}`).toBeGreaterThanOrEqual(4.5);
    }
  });
  it("éléments graphiques : bords de contrôles ≥ 3:1, texte sur bouton principal ≥ 4,5:1", () => {
    for (const s of surfaces) expect(contrast(token("line-strong"), token(s)), `line-strong sur ${s}`).toBeGreaterThanOrEqual(3);
    expect(contrast(token("on-accent"), token("accent"))).toBeGreaterThanOrEqual(4.5);
    expect(contrast(token("on-accent"), token("accent-hover"))).toBeGreaterThanOrEqual(4.5);
  });
  it("6 tailles de texte au plus, toutes en jetons", () => {
    expect([...rules.matchAll(/font-size:\s*([^;}]+)/g)].map((m) => m[1]!.trim()).filter((v) => !/^var\(--fs-(xs|sm|md|lg|xl|hero)\)$/.test(v) && v !== "inherit")).toEqual([]);
  });
  it("une seule échelle d'espacement (grille de 4)", () => {
    const allowed = new Set([0, 2, 4, 8, 12, 16, 24, 32, 48]);
    const bad: string[] = [];
    for (const m of rules.matchAll(/(?<![-\w])(?:padding|margin|gap|row-gap|column-gap)(?:-(?:top|bottom|left|right))?\s*:\s*([^;}]+)/g)) {
      for (const px of m[1]!.matchAll(/-?(\d+(?:\.\d+)?)px/g)) if (!allowed.has(Number(px[1]))) bad.push(m[0]);
    }
    expect(bad).toEqual([]);
  });
  it("3 rayons au plus", () => {
    const vals = new Set([...rules.matchAll(/border-radius:\s*([^;}]+)/g)].flatMap((m) => m[1]!.trim().split(/\s+/)));
    for (const v of vals) expect(["var(--r-ctl)", "var(--r-card)", "var(--r-pill)", "0"], v).toContain(v);
  });
  it("aucune couleur écrite en dur hors des jetons (CSS et composants)", () => {
    expect(rules.match(/#[0-9a-fA-F]{3,8}\b|rgba?\(/g) ?? []).toEqual([]);
  });
  it("mouvement réductible", () => {
    expect(rules).toContain("prefers-reduced-motion");
  });
});

/** Fichiers TSX encore à migrer vers les jetons : vide depuis la 0.53, à garder vide. */
const PENDING = new Set<string>([]);
const walk = (d: string): string[] => readdirSync(d).flatMap((f: string) => (statSync(join(d, f)).isDirectory() ? walk(join(d, f)) : [join(d, f)]));
describe("couleurs dans les composants", () => {
  it("aucune couleur en dur hors des fichiers encore à migrer", () => {
    const offenders = walk(here)
      .filter((f) => f.endsWith(".tsx") && !f.includes("preview"))
      .map((f) => f.slice(here.length + 1).replace(/\\/g, "/"))
      .filter((f) => !PENDING.has(f) && /#[0-9a-fA-F]{3,8}\b|rgba?\(/.test(readFileSync(join(here, f), "utf8")));
    expect(offenders).toEqual([]);
  });
});

describe("tailles de texte dans les composants", () => {
  // Les textes de la carte de piste sont à l'échelle du zoom (« 11 * mPerPx ») : pas des tailles d'écran.
  it("les tailles écrites dans le code restent sur l'échelle (12, 14, 16, 20, 28, 48)", () => {
    const scale = new Set([12, 14, 16, 20, 28, 48]);
    const bad = walk(here)
      .filter((f) => f.endsWith(".tsx") && !f.includes("preview"))
      .flatMap((f) => [...readFileSync(f, "utf8").matchAll(/fontSize[:=]\s*\{?\s*(\d+(?:\.\d+)?)\s*[},]/g)].map((m) => `${f.slice(here.length + 1)} : ${m[1]}`))
      .filter((x) => !scale.has(Number(x.split(" : ")[1])));
    expect(bad).toEqual([]);
  });
});
