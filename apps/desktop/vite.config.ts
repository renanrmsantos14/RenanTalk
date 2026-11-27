import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Porta exclusiva do RenanTalk (registrada em ~/.codex/dev-ports.json); `strictPort` falha em vez de trocar em silêncio.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 36495, strictPort: true },
  build: { target: "es2022", outDir: "dist", emptyOutDir: true },
});
