import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// Synthèse pilotée à la main : chaque appel rend une promesse qu'on résout quand on veut.
const pending: Array<(v: Uint8Array) => void> = [];
vi.mock("./api", () => ({
  isTauri: () => true,
  api: { ttsSpeak: () => new Promise<Uint8Array>((res) => pending.push(res)) },
}));

const played: string[] = [];
class FakeAudio {
  src: string;
  volume = 1;
  constructor(src: string) {
    this.src = src;
  }
  addEventListener() {}
  pause() {}
  play() {
    played.push(this.src);
    return Promise.resolve();
  }
}

describe("voix du coach", () => {
  let n = 0;
  beforeEach(() => {
    pending.length = 0;
    played.length = 0;
    vi.stubGlobal("Audio", FakeAudio);
    vi.stubGlobal("URL", { createObjectURL: () => `blob:${++n}`, revokeObjectURL: () => undefined });
  });
  afterEach(() => vi.unstubAllGlobals());

  it("un message plus ancien dont la synthèse finit en dernier n'est pas joué", async () => {
    const { speak } = await import("./voice");
    const a = speak("A", "piper:x");
    const b = speak("B", "piper:x");
    pending[1]!(new Uint8Array([2]));
    await b;
    pending[0]!(new Uint8Array([1]));
    await a;
    expect(played).toHaveLength(1);
  });

  it("radio enfoncée pendant la synthèse : rien n'est joué", async () => {
    const { speak, stopVoice } = await import("./voice");
    const a = speak("A", "piper:x");
    stopVoice();
    pending[0]!(new Uint8Array([1]));
    await a;
    expect(played).toHaveLength(0);
  });
});
