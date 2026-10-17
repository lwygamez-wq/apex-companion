import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { fileURLToPath } from "node:url";

// App mobile (iPhone) : page web installable, publiée sur GitHub Pages.
// Elle ouvre les exports .apex.json.gz localement : aucune donnée n'est envoyée.
export default defineConfig({
  root: fileURLToPath(new URL("./mobile", import.meta.url)),
  base: "./",
  plugins: [react()],
  build: { outDir: fileURLToPath(new URL("./dist-mobile", import.meta.url)), emptyOutDir: true, target: "safari15", sourcemap: false },
  server: { port: 1421, fs: { allow: [".."] } },
});
