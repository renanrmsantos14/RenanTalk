// Contrato com o núcleo Rust. No navegador (sem Tauri, `pnpm dev`) usa dados de exemplo para desenvolver a tela.
import { invoke } from "@tauri-apps/api/core";
import { mockInvoke } from "./mock";

export const emTauri = "__TAURI_INTERNALS__" in window;

/** Chama um comando do `src-tauri/src/tela.rs`. Nunca registre `args` (pode levar a chave Groq). */
export function chamar<T = any>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  return emTauri ? invoke<T>(cmd, args) : (mockInvoke(cmd, args) as Promise<T>);
}
