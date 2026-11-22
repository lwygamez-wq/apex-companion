// Types minimaux de node:fs pour les tests Vitest (le projet n'embarque pas @types/node).
declare module "node:fs" {
  export function readFileSync(path: string, encoding: "utf8"): string;
  export function readdirSync(path: string): string[];
  export function statSync(path: string): { isDirectory(): boolean };
}
