import { describe, expect, it } from "vitest";
import { alpha, hexAlpha, mix, tok } from "./palette";

describe("palette", () => {
  it("convertit un jeton hexadécimal en rgba pour les canvas", () => {
    expect(hexAlpha("#3dd9b3", 0.18)).toBe("rgba(61, 217, 179, 0.18)");
    expect(hexAlpha("gray", 0.5)).toBe("gray");
  });
  it("hors navigateur, repli neutre", () => {
    expect(tok("loss")).toBe("gray");
    expect(alpha("loss", 0.2)).toBe("gray");
  });
  it("SVG : color-mix sur la variable", () => {
    expect(mix("loss", 0.3)).toBe("color-mix(in srgb, var(--loss) 30%, transparent)");
  });
});
