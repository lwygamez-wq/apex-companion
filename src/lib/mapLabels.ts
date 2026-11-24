// Placement des étiquettes de virage sur la carte, en pixels écran : aucune étiquette ne doit en
// chevaucher une autre ni sortir du cadre. Pur et testable (Vitest), utilisé par TrackMap.

export interface LabelIn {
  id: number;
  /** Point de la trajectoire (apex), en pixels écran. */
  x: number;
  y: number;
  /** Normale vers l'extérieur du virage (unitaire, repère écran). */
  nx: number;
  ny: number;
  /** Taille de la pastille seule (numéro) et de l'étiquette complète (numéro + nom). */
  small: { w: number; h: number };
  big: { w: number; h: number };
  /** Plus grand = placé en premier (virages prioritaires, plus grosse perte). */
  priority: number;
}

export interface LabelOut {
  id: number;
  /** Coin haut gauche de la boîte retenue. */
  x: number;
  y: number;
  w: number;
  h: number;
  /** Le nom tient à côté du numéro. */
  named: boolean;
  /** Centre de la pastille (pour le trait qui la relie à la trajectoire). */
  cx: number;
  cy: number;
}

export interface Box { x: number; y: number; w: number; h: number }

export const overlaps = (a: Box, b: Box, pad = 0): boolean => a.x < b.x + b.w + pad && b.x < a.x + a.w + pad && a.y < b.y + b.h + pad && b.y < a.y + a.h + pad;

/** Angles essayés autour de la normale extérieure (degrés), puis côté intérieur en dernier recours. */
const ANGLES = [0, 30, -30, 60, -60, 90, -90, 135, -135, 180];
const DISTS = [22, 34, 50, 70];

/**
 * Place les étiquettes par priorité décroissante. Pour chacune, essaie d'abord l'étiquette complète
 * (numéro + nom) à plusieurs angles et distances de la trajectoire, puis la pastille seule.
 * Une étiquette qui ne trouve aucune place libre n'est pas affichée (absente du résultat) :
 * mieux vaut un virage sans repère qu'un repère illisible.
 * `obstacles` : zones réservées (boutons, cartouches posés sur la carte), jamais recouvertes.
 * `soft` : zones à éviter si possible (la trajectoire) ; une étiquette les recouvre plutôt que de disparaître.
 */
export function placeLabels(items: LabelIn[], width: number, height: number, obstacles: Box[] = [], pad = 3, soft: Box[] = []): LabelOut[] {
  const placed: LabelOut[] = [];
  const taken: Box[] = [...obstacles];
  let avoidSoft = true;
  const free = (b: Box) => b.x >= 2 && b.y >= 2 && b.x + b.w <= width - 2 && b.y + b.h <= height - 2 && !taken.some((t) => overlaps(b, t, pad)) && !(avoidSoft && soft.some((t) => overlaps(b, t, 0)));
  const order = [...items].sort((a, b) => b.priority - a.priority);
  for (const it of order) {
    let done: LabelOut | null = null;
    for (const [size, named, soft_] of [[it.big, true, true], [it.small, false, true], [it.big, true, false], [it.small, false, false]] as const) {
      avoidSoft = soft_;
      for (const d of DISTS) {
        for (const deg of ANGLES) {
          const a = (deg * Math.PI) / 180;
          const dx = it.nx * Math.cos(a) - it.ny * Math.sin(a);
          const dy = it.nx * Math.sin(a) + it.ny * Math.cos(a);
          // La pastille (carré de hauteur h) est au bout du rayon ; le nom s'étend du côté où l'on s'éloigne.
          const pcx = it.x + dx * d, pcy = it.y + dy * d;
          const r = size.h / 2;
          const left = named && dx < -0.2;
          const x = left ? pcx + r - size.w : pcx - r;
          const b: Box = { x, y: pcy - r, w: size.w, h: size.h };
          if (free(b)) {
            done = { id: it.id, ...b, named, cx: pcx, cy: pcy };
            break;
          }
        }
        if (done) break;
      }
      if (done) break;
    }
    if (done) {
      placed.push(done);
      taken.push(done);
    }
  }
  return placed;
}
