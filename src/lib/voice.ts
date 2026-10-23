// Voix du coach : synthèse vocale de Windows (WebView2), voix française si elle est installée.

let cached: SpeechSynthesisVoice | null = null;

function frenchVoice(): SpeechSynthesisVoice | null {
  if (typeof window === "undefined" || !("speechSynthesis" in window)) return null;
  if (cached) return cached;
  const voices = window.speechSynthesis.getVoices();
  // Voix naturelles d'abord (Windows 11 : « Microsoft Denise/Henri Online (Natural) »), puis toute voix française.
  cached =
    voices.find((v) => v.lang.toLowerCase().startsWith("fr") && /natural|online/i.test(v.name)) ??
    voices.find((v) => v.lang.toLowerCase() === "fr-fr") ??
    voices.find((v) => v.lang.toLowerCase().startsWith("fr")) ??
    null;
  return cached;
}

if (typeof window !== "undefined" && "speechSynthesis" in window) {
  window.speechSynthesis.onvoiceschanged = () => {
    cached = null;
    frenchVoice();
  };
}

export function voiceAvailable(): boolean {
  return typeof window !== "undefined" && "speechSynthesis" in window;
}

export function voiceName(): string | null {
  return frenchVoice()?.name ?? null;
}

/** Prononce un message radio ; un nouveau message coupe le précédent (le plus récent est le seul utile). */
export function speak(text: string): void {
  if (!voiceAvailable()) return;
  const u = new SpeechSynthesisUtterance(text);
  const v = frenchVoice();
  if (v) u.voice = v;
  u.lang = v?.lang ?? "fr-FR";
  u.rate = 1.08;
  u.pitch = 1;
  window.speechSynthesis.cancel();
  window.speechSynthesis.speak(u);
}
