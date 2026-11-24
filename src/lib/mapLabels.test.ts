import { describe, expect, it } from "vitest";
import { overlaps, placeLabels, type LabelIn } from "./mapLabels";

const item = (id: number, x: number, y: number, nx = 0, ny = -1, priority = 0, name = 80): LabelIn => ({ id, x, y, nx, ny, small: { w: 24, h: 24 }, big: { w: 24 + name, h: 24 }, priority });

describe("placement des étiquettes de virage", () => {
  it("place une étiquette seule à l'extérieur du virage, avec son nom", () => {
    const [l] = placeLabels([item(1, 200, 200)], 400, 400);
    expect(l!.named).toBe(true);
    expect(l!.cy).toBeLessThan(200); // normale vers le haut
  });

  it("n'en superpose jamais deux, même pour des virages collés (chicane)", () => {
    const items = [item(1, 200, 200), item(2, 206, 204), item(3, 212, 198, 1, 0), item(4, 190, 210, -1, 0), item(5, 203, 195, 0, 1)];
    const out = placeLabels(items, 420, 420);
    expect(out.length).toBe(5);
    for (let i = 0; i < out.length; i++) for (let j = i + 1; j < out.length; j++) expect(overlaps(out[i]!, out[j]!), `${out[i]!.id} × ${out[j]!.id}`).toBe(false);
  });

  it("place d'abord les virages prioritaires, et retire le nom avant de retirer le virage", () => {
    // 12 virages sur 120 px : il n'y a pas la place pour tous les noms.
    const items = Array.from({ length: 12 }, (_, i) => item(i, 60 + i * 10, 60, 0, -1, i === 7 ? 10 : 0));
    const out = placeLabels(items, 260, 140);
    expect(out.find((l) => l.id === 7)?.named).toBe(true);
    expect(out.some((l) => !l.named)).toBe(true);
    for (let i = 0; i < out.length; i++) for (let j = i + 1; j < out.length; j++) expect(overlaps(out[i]!, out[j]!)).toBe(false);
  });

  it("évite les zones réservées et reste dans le cadre", () => {
    const out = placeLabels([item(1, 10, 10, -1, -1)], 300, 300, [{ x: 0, y: 40, w: 60, h: 60 }]);
    const l = out[0]!;
    expect(l.x).toBeGreaterThanOrEqual(2);
    expect(l.y).toBeGreaterThanOrEqual(2);
    expect(overlaps(l, { x: 0, y: 40, w: 60, h: 60 })).toBe(false);
  });

  it("évite la trajectoire quand c'est possible, sans jamais retirer un virage pour elle", () => {
    const track = Array.from({ length: 40 }, (_, i) => ({ x: 100 + i * 5 - 5, y: 195, w: 10, h: 10 }));
    const [l] = placeLabels([item(1, 200, 200)], 400, 400, [], 3, track);
    expect(track.some((t) => overlaps(l!, t))).toBe(false);
    // Piste partout : l'étiquette est quand même posée.
    const everywhere = [{ x: 0, y: 0, w: 400, h: 400 }];
    expect(placeLabels([item(1, 200, 200)], 400, 400, [], 3, everywhere).length).toBe(1);
  });
});
