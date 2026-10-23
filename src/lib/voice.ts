// Voix du coach : voix naturelle hors ligne (Piper, téléchargée à la demande) ou synthèse de Windows.
import { api, isTauri } from "./api";

let cached: SpeechSynthesisVoice | null = null;
let current = "windows";
let volume = 0.5;
let playing: HTMLAudioElement | null = null;

function frenchVoice(): SpeechSynthesisVoice | null {
  if (typeof window === "undefined" || !("speechSynthesis" in window)) return null;
  if (cached) return cached;
  const voices = window.speechSynthesis.getVoices();
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

export function setCoachVoice(id: string | null | undefined): void {
  current = id || "windows";
}

/** Volume de 0 à 100 ; s'applique aussi au message en cours. */
export function setCoachVolume(v: number | null | undefined): void {
  volume = Math.max(0, Math.min(100, v ?? 50)) / 100;
  if (playing) playing.volume = volume;
}

export function coachVolume(): number {
  return Math.round(volume * 100);
}

export function coachVoice(): string {
  return current;
}

export function voiceAvailable(): boolean {
  return typeof window !== "undefined" && "speechSynthesis" in window;
}

export function voiceName(): string | null {
  return frenchVoice()?.name ?? null;
}

function speakWindows(text: string): void {
  if (!voiceAvailable()) return;
  const u = new SpeechSynthesisUtterance(text);
  const v = frenchVoice();
  if (v) u.voice = v;
  u.lang = v?.lang ?? "fr-FR";
  u.rate = 1.08;
  u.volume = volume;
  window.speechSynthesis.cancel();
  window.speechSynthesis.speak(u);
}

function stop(): void {
  if (playing) {
    playing.pause();
    URL.revokeObjectURL(playing.src);
    playing = null;
  }
  if (voiceAvailable()) window.speechSynthesis.cancel();
}

/**
 * Prononce un message radio ; un nouveau message coupe le précédent (le plus récent est le seul utile).
 * Voix naturelle en cas d'échec (pas encore téléchargée, hors Windows) : repli sur la voix de Windows.
 */
export async function speak(text: string, voice: string = current): Promise<void> {
  stop();
  if (voice === "windows" || !isTauri()) return speakWindows(text);
  try {
    const wav = await api.ttsSpeak(voice, text);
    const a = new Audio(URL.createObjectURL(new Blob([wav], { type: "audio/wav" })));
    a.volume = volume;
    playing = a;
    await a.play();
  } catch (e) {
    console.warn("voix naturelle indisponible, repli sur Windows :", e);
    speakWindows(text);
  }
}
