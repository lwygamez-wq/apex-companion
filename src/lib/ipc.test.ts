import { describe, expect, it } from "vitest";
import libRs from "../../src-tauri/src/lib.rs?raw";
import commandsRs from "../../src-tauri/src/commands.rs?raw";

// Contrat IPC : chaque commande appelée par l'interface doit être enregistrée côté Tauri
// (`generate_handler!` dans src-tauri/src/lib.rs) et définie dans commands.rs. Une commande oubliée
// ne casse ni la compilation ni les types : elle échoue seulement dans l'app, au clic.

const sources = import.meta.glob(["../**/*.{ts,tsx}", "!../**/*.test.{ts,tsx}", "!../preview*"], { query: "?raw", import: "default", eager: true }) as Record<string, string>;

function calledCommands(): Set<string> {
  const names = new Set<string>();
  for (const text of Object.values(sources)) {
    for (const m of text.matchAll(/\b(?:call|invoke)(?:<[^(]*>)?\(\s*"([a-z0-9_]+)"/g)) if (m[1]) names.add(m[1]);
  }
  return names;
}

function registeredCommands(): Set<string> {
  const block = /generate_handler!\[([\s\S]*?)\]/.exec(libRs)?.[1] ?? "";
  return new Set([...block.matchAll(/commands::([a-z0-9_]+)/g)].flatMap((m) => (m[1] ? [m[1]] : [])));
}

describe("contrat IPC interface ↔ Tauri", () => {
  const called = calledCommands();
  const registered = registeredCommands();

  it("trouve les commandes des deux côtés", () => {
    expect(called.size).toBeGreaterThan(30);
    expect(registered.size).toBeGreaterThan(30);
  });

  it("chaque commande appelée est enregistrée et définie", () => {
    const missing = [...called].filter((c) => !registered.has(c));
    expect(missing, "commandes appelées mais absentes de generate_handler!").toEqual([]);
    const undefinedFns = [...registered].filter((c) => !new RegExp(`pub (async )?fn ${c}\\b`).test(commandsRs));
    expect(undefinedFns, "commandes enregistrées sans fonction dans commands.rs").toEqual([]);
  });
});
