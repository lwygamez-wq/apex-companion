import type { Comparison } from "./types";

const fr = (x: number, d = 1) => x.toFixed(d).replace(".", ",");

/** Les virages (2 au plus) qui expliquent l'écart : plus de 0,1 s, du plus coûteux au moins coûteux. */
export function worstCorners(c: Comparison, sign: 1 | -1): string[] {
  return c.corners
    .filter((k) => sign * k.time_delta_s > 0.1)
    .sort((a, b) => sign * (b.time_delta_s - a.time_delta_s))
    .slice(0, 2)
    .map((k) => k.corner.name);
}

function article(names: string[]): string {
  if (names.length > 1) return /^virages /.test(list(names)) ? "aux " : "à ";
  return /^(Virage|V)\s*\S+$/i.test(names[0]!) ? "au " : "à ";
}

function list(names: string[]): string {
  if (names.length < 2) return names[0] ?? "";
  // « Virage 1 et Virage 6 » → « virages 1 et 6 » quand les deux s'appellent « Virage N ».
  const nums = names.map((n) => /^(?:Virage|V)\s*(\S+)$/i.exec(n)?.[1]);
  if (nums.every(Boolean)) return `virages ${nums[0]} et ${nums[1]}`;
  return `${names[0]} et ${names[1]}`;
}

/**
 * Phrase-résumé de la comparaison, en langage de pilote :
 * « Ton tour 16 a perdu 1,7 s sur ton meilleur, surtout aux virages 1 et 6. »
 * Renvoie les morceaux pour colorer le chiffre et les virages.
 */
export function headline(c: Comparison, gapS: number | null): { lead: string; amount: string | null; mid: string; where: string | null; tail: string } {
  const who = `Ton tour ${c.compared.number}`;
  const vs = c.reference.driver ? `sur ${c.reference.driver}` : c.reference.lap_id === c.compared.lap_id ? "" : "sur ta référence";
  if (gapS == null || Math.abs(gapS) < 0.05) {
    return { lead: `${who} est au niveau de ta référence`, amount: null, mid: "", where: null, tail: "." };
  }
  const lost = gapS > 0;
  const names = worstCorners(c, lost ? 1 : -1);
  const where = names.length ? list(names) : null;
  return {
    lead: `${who} a ${lost ? "perdu" : "gagné"} `,
    amount: `${fr(Math.abs(gapS))} s`,
    mid: ` ${vs}${where ? `, surtout ${article(names)}` : ""}`,
    where: where ? (names.length === 1 ? where.replace(/^Virage/, "virage") : where) : null,
    tail: ".",
  };
}
