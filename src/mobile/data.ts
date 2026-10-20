// Lecture des exports .apex.json.gz et mémorisation locale (IndexedDB, dans le téléphone uniquement).
import type { Comparison, Consistency, LapRow, OffTrackZone, Resampled, SessionMeta } from "../lib/types";

export interface MobileSession {
  key: string;
  meta: SessionMeta;
  laps: LapRow[];
  best_lap_id: number | null;
  reference_lap_id: number | null;
  fastest_lap_id: number | null;
  off_tracks: OffTrackZone[];
  theoretical_best_s: number | null;
  consistency: Consistency | null;
  comparisons: Comparison[];
  app_version: string;
}

export interface StoredSummary {
  key: string;
  track: string;
  car: string;
  started_at_unix: number;
  best_lap_s: number | null;
  lap_count: number;
  added_at: number;
}

async function gunzip(buf: ArrayBuffer): Promise<string> {
  const b = new Uint8Array(buf);
  if (b[0] === 0x1f && b[1] === 0x8b) {
    if (typeof DecompressionStream === "undefined") throw new Error("Ce navigateur ne sait pas décompresser les exports (iOS 16.4 minimum).");
    const stream = new Blob([b]).stream().pipeThrough(new DecompressionStream("gzip"));
    return await new Response(stream).text();
  }
  return new TextDecoder().decode(b);
}

export async function parseExport(buf: ArrayBuffer): Promise<MobileSession> {
  let doc: Record<string, unknown>;
  try {
    doc = JSON.parse(await gunzip(buf)) as Record<string, unknown>;
  } catch (e) {
    throw new Error(`Fichier illisible : ce n'est pas un export Apex (${e instanceof Error ? e.message : e}).`);
  }
  if (doc.format !== "apex-session-export") throw new Error("Ce fichier n'est pas un export de session Apex Companion.");
  if (!Array.isArray(doc.comparisons)) {
    throw new Error("Export d'une ancienne version : mets à jour Apex Companion sur le PC (0.6 ou plus) puis exporte à nouveau la session.");
  }
  const refTrace = doc.reference_trace as Resampled;
  const comparisons = (doc.comparisons as Comparison[]).map((c) => ({ ...c, ref_trace: refTrace, ref_off_track_m: c.ref_off_track_m ?? [], cmp_off_track_m: c.cmp_off_track_m ?? [] }));
  // Les traces brutes ne servent pas sur mobile : on ne garde que les métadonnées des tours.
  const laps = (doc.laps as (LapRow & { trace?: unknown })[]).map(({ trace: _t, ...l }) => l as LapRow);
  const meta = doc.session as SessionMeta;
  return {
    key: meta.source_fingerprint,
    meta,
    laps,
    best_lap_id: (doc.best_lap_id as number | null) ?? null,
    reference_lap_id: (doc.reference_lap_id as number | null) ?? null,
    fastest_lap_id: (doc.fastest_lap_id as number | null) ?? null,
    off_tracks: (doc.off_tracks as OffTrackZone[] | undefined) ?? [],
    theoretical_best_s: (doc.theoretical_best_s as number | null) ?? null,
    consistency: (doc.consistency as Consistency | null) ?? null,
    comparisons,
    app_version: String(doc.app_version ?? ""),
  };
}

export function summarize(s: MobileSession): StoredSummary {
  const best = s.laps.find((l) => l.id === (s.best_lap_id ?? s.fastest_lap_id))?.lap_time_s ?? null;
  return { key: s.key, track: s.meta.track, car: s.meta.car, started_at_unix: s.meta.started_at_unix, best_lap_s: best, lap_count: s.laps.length, added_at: Date.now() };
}

// --- IndexedDB -----------------------------------------------------------------
const DB = "apex-mobile";
function open(): Promise<IDBDatabase> {
  return new Promise((res, rej) => {
    const r = indexedDB.open(DB, 1);
    r.onupgradeneeded = () => {
      r.result.createObjectStore("summaries", { keyPath: "key" });
      r.result.createObjectStore("files");
    };
    r.onsuccess = () => res(r.result);
    r.onerror = () => rej(r.error);
  });
}
function tx<T>(stores: string[], mode: IDBTransactionMode, fn: (t: IDBTransaction) => IDBRequest<T> | void): Promise<T | undefined> {
  return open().then(
    (db) =>
      new Promise((res, rej) => {
        const t = db.transaction(stores, mode);
        const req = fn(t);
        t.oncomplete = () => res(req ? req.result : undefined);
        t.onerror = () => rej(t.error);
      }),
  );
}

export async function saveSession(s: MobileSession, file: ArrayBuffer): Promise<void> {
  await tx(["summaries", "files"], "readwrite", (t) => {
    t.objectStore("summaries").put(summarize(s));
    t.objectStore("files").put(file, s.key);
  });
}
export async function listSessions(): Promise<StoredSummary[]> {
  try {
    const all = ((await tx(["summaries"], "readonly", (t) => t.objectStore("summaries").getAll())) ?? []) as StoredSummary[];
    return all.sort((a, b) => b.started_at_unix - a.started_at_unix);
  } catch {
    return [];
  }
}
export async function loadSession(key: string): Promise<MobileSession> {
  const f = (await tx(["files"], "readonly", (t) => t.objectStore("files").get(key))) as ArrayBuffer | undefined;
  if (!f) throw new Error("Session introuvable sur ce téléphone : ouvre à nouveau le fichier.");
  return parseExport(f);
}
export async function deleteSession(key: string): Promise<void> {
  await tx(["summaries", "files"], "readwrite", (t) => {
    t.objectStore("summaries").delete(key);
    t.objectStore("files").delete(key);
  });
}
