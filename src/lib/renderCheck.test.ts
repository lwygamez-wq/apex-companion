import { describe, expect, it } from "vitest";
import { engineVersion, sameColor } from "./renderCheck";

describe("auto-test d'affichage", () => {
  it("lit la version de WebView2 dans l'agent utilisateur", () => {
    expect(engineVersion("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36 Edg/131.0.2903.86")).toBe("WebView2 / Edge 131.0.2903.86");
    expect(engineVersion("Mozilla/5.0 (X11; Linux x86_64) Chrome/140.0.0.0 Safari/537.36")).toBe("Chromium 140.0.0.0");
    expect(engineVersion("inconnu")).toBe("moteur inconnu");
  });
  it("compare deux couleurs calculées sans tenir compte des espaces", () => {
    expect(sameColor("rgb(255, 122, 31)", "rgb(255,122,31)")).toBe(true);
    expect(sameColor("rgb(0, 0, 0)", "rgb(255, 122, 31)")).toBe(false);
    expect(sameColor("", "")).toBe(false);
  });
});
