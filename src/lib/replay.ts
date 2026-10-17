// Relecture d'un tour : horloge partagée entre la carte, les courbes et le panneau de lecture.
// Stockée hors de React (abonnement direct) pour ne pas re-rendre toute la page 60 fois par seconde.
import { useSyncExternalStore } from "react";
import type { Comparison, Resampled } from "./types";

export interface ReplayState {
  /** Temps écoulé depuis le début du tour analysé (s). */
  t: number;
  playing: boolean;
  speed: number;
  /** Les courbes affichent une fenêtre glissante autour de la voiture. */
  chartFollow: boolean;
  /** Dernier déplacement venu du survol d'une courbe (la fenêtre des courbes ne bouge pas). */
  fromChart: boolean;
}

export class ReplayStore {
  private s: ReplayState = { t: 0, playing: false, speed: 1, chartFollow: true, fromChart: false };
  private subs = new Set<() => void>();
  private raf = 0;
  constructor(public readonly start: number, public readonly end: number) {
    this.s = { ...this.s, t: start };
  }
  get = (): ReplayState => this.s;
  subscribe = (cb: () => void): (() => void) => {
    this.subs.add(cb);
    return () => this.subs.delete(cb);
  };
  private set(p: Partial<ReplayState>) {
    this.s = { ...this.s, ...p };
    this.subs.forEach((f) => f());
  }
  seek(t: number, fromChart = false) {
    this.set({ t: Math.min(this.end, Math.max(this.start, t)), fromChart });
  }
  setChartFollow(chartFollow: boolean) {
    this.set({ chartFollow });
  }
  setSpeed(speed: number) {
    this.set({ speed });
  }
  play() {
    if (this.s.playing) return;
    if (this.s.t >= this.end - 0.01) this.set({ t: this.start });
    this.set({ playing: true });
    let last = performance.now();
    const tick = (now: number) => {
      if (!this.s.playing) return;
      const t = this.s.t + ((now - last) / 1000) * this.s.speed;
      last = now;
      if (t >= this.end) {
        this.set({ t: this.end, playing: false });
        return;
      }
      this.set({ t, fromChart: false });
      this.raf = requestAnimationFrame(tick);
    };
    this.raf = requestAnimationFrame(tick);
  }
  pause() {
    cancelAnimationFrame(this.raf);
    if (this.s.playing) this.set({ playing: false });
  }
  toggle() {
    if (this.s.playing) this.pause();
    else this.play();
  }
  dispose() {
    cancelAnimationFrame(this.raf);
    this.subs.clear();
  }
}

export function useReplay(store: ReplayStore): ReplayState {
  return useSyncExternalStore(store.subscribe, store.get);
}

/** Index fractionnaire où `time` (croissant) atteint `t`. */
export function indexAtTime(time: number[], t: number): number {
  const n = time.length;
  if (n === 0) return 0;
  if (t <= time[0]!) return 0;
  if (t >= time[n - 1]!) return n - 1;
  let lo = 0;
  let hi = n - 1;
  while (hi - lo > 1) {
    const mid = (lo + hi) >> 1;
    if (time[mid]! <= t) lo = mid;
    else hi = mid;
  }
  const a = time[lo]!;
  const b = time[hi]!;
  return lo + (b > a ? (t - a) / (b - a) : 0);
}

/** Valeur interpolée à un index fractionnaire. */
export function at(arr: number[] | null | undefined, fi: number): number | null {
  if (!arr || arr.length === 0) return null;
  const i = Math.max(0, Math.min(arr.length - 1, fi));
  const lo = Math.floor(i);
  const hi = Math.min(arr.length - 1, lo + 1);
  return arr[lo]! + (arr[hi]! - arr[lo]!) * (i - lo);
}

/** Bornes de lecture : du premier au dernier point couvert par le tour analysé. */
export function replayBounds(c: Comparison): [number, number] {
  const t = c.cmp_trace.time;
  return [t[0] ?? 0, t[t.length - 1] ?? 0];
}

/** Positions à l'instant `t` : voiture analysée et fantôme de la référence (au même temps). */
export function positionsAt(c: Comparison, t: number): { ci: number; ri: number } {
  return { ci: indexAtTime(c.cmp_trace.time, t), ri: indexAtTime(c.ref_trace.time, t) };
}

export function hasPositions(r: Resampled): boolean {
  return !!r.x && !!r.y && r.x.length > 2;
}
