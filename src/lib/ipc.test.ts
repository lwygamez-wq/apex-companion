import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

// Contrat IPC : chaque commande appelée par l'interface doit être enregistrée côté Tauri
// (`generate_handler!` dans src-tauri/src/lib.rs) et définie dans commands.rs. Une commande oubliée
// ne casse ni la compilation ni les types : elle échoue seulement dans l'app, au clic.

const root = join(__dirname, "..", "..");

function files(dir: string): string[] {
  return readdirSync(dir).flatMap((f) => {
    const p = join(dir, f);
    if (statSync(p).isDirectory()) return files(p);
    return /\.(ts|tsx)$/.test(f) && !/\.test\.tsx?$/.test(f) && !f.startsWith("preview") ? [p] : [];
  });
}

function calledCommands(): Set<string> {
  const names = new Set<string>();
  for (const f of files(join(root, "src"))) {
    for (const m of readFileSync(f, "utf8").matchAll(/\b(?:call|invoke)(?:<[^(]*>)?\(\s*"([a-z0-9_]+)"/g)) names.add(m[1]);
  }
  return names;
}

function registeredCommands(): Set<string> {
  const lib = readFileSync(join(root, "src-tauri", "src", "lib.rs"), "utf8");
  const block = /generate_handler!\[([\s\S]*?)\]/.exec(lib)?.[1] ?? "";
  return new Set([...block.matchAll(/commands::([a-z0-9_]+)/g)].map((m) => m[1]));
}

describe("contrat IPC interface ↔ Tauri", () => {
  const called = calledCommands();
  const registered = registeredCommands();
  const defined = readFileSync(join(root, "src-tauri", "src", "commands.rs"), "utf8");

  it("trouve les commandes des deux côtés", () => {
    expect(called.size).toBeGreaterThan(30);
    expect(registered.size).toBeGreaterThan(30);
  });

  it("chaque commande appelée est enregistrée et définie", () => {
    const missing = [...called].filter((c) => !registered.has(c));
    expect(missing, "commandes appelées mais absentes de generate_handler!").toEqual([]);
    const undefinedFns = [...registered].filter((c) => !new RegExp(`pub (async )?fn ${c}\\b`).test(defined));
    expect(undefinedFns, "commandes enregistrées sans fonction dans commands.rs").toEqual([]);
  });
});
