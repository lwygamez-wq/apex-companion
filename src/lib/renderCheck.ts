// Auto-test d'affichage : vérifie, dans le moteur qui affiche l'app (WebView2 sous Windows), ce que la refonte
// suppose sans l'avoir vu sur le PC d'Alex : color-mix, :has(), var() dans les SVG et les polices embarquées.

export interface RenderCheck {
  id: string;
  label: string;
  ok: boolean;
  /** Ce qui a été mesuré (valeur calculée, version), pour la capture envoyée. */
  detail: string;
}

/** Version du moteur lue dans l'agent utilisateur (« Edg/131.0… » pour WebView2, sinon Chrome). */
export function engineVersion(ua: string): string {
  const m = /Edg\/([\d.]+)/.exec(ua) ?? /Chrome\/([\d.]+)/.exec(ua);
  return m ? `${ua.includes("Edg/") ? "WebView2 / Edge" : "Chromium"} ${m[1]}` : "moteur inconnu";
}

/** Deux couleurs calculées sont-elles la même (format rgb(...) du navigateur) ? */
export function sameColor(a: string, b: string): boolean {
  const n = (s: string) => s.replace(/\s+/g, "").toLowerCase();
  return a !== "" && n(a) === n(b);
}

/** Lance les vérifications dans la page (navigateur seulement). */
export function runRenderChecks(): RenderCheck[] {
  const out: RenderCheck[] = [];
  const supports = (k: string, v?: string) => {
    try {
      return v === undefined ? CSS.supports(k) : CSS.supports(k, v);
    } catch {
      return false;
    }
  };
  out.push({ id: "color-mix", label: "Couleurs mélangées (color-mix)", ok: supports("color", "color-mix(in srgb, red 50%, transparent)"), detail: "" });
  out.push({ id: "has", label: "Bandeaux de titre (:has)", ok: supports("selector(:has(> a))"), detail: "" });

  // var() dans un attribut SVG : la couleur calculée doit être celle du jeton.
  const box = document.createElement("div");
  box.style.cssText = "position:absolute;left:-9999px;top:0";
  box.innerHTML = '<svg width="4" height="4"><rect width="4" height="4" fill="var(--accent)"/></svg><span style="color:var(--accent)">x</span>';
  document.body.appendChild(box);
  const fill = getComputedStyle(box.querySelector("rect")!).fill;
  const ref = getComputedStyle(box.querySelector("span")!).color;
  box.remove();
  out.push({ id: "svg-var", label: "Couleurs des dessins (var() dans les SVG)", ok: sameColor(fill, ref), detail: `${fill} / ${ref}` });

  for (const [id, fam, label] of [
    ["font-ui", "Saira Variable", "Police de l'interface (Saira)"],
    ["font-board", "Big Shoulders Display Variable", "Police des panneaux (Big Shoulders)"],
  ] as const) {
    const ok = document.fonts.check(`16px "${fam}"`) && [...document.fonts].some((f) => f.family.replace(/"/g, "") === fam && f.status === "loaded");
    out.push({ id, label, ok, detail: "" });
  }
  out.push({ id: "engine", label: "Moteur d'affichage", ok: true, detail: engineVersion(navigator.userAgent) });
  return out;
}
