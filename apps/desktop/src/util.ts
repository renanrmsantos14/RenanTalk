import { useCallback, useRef, useState } from "react";

export const NOME_IDIOMA: Record<string, string> = { pt: "Português", en: "Inglês", auto: "Automático" };
export const NOME_MODO: Record<string, string> = { literal: "Literal", revisado: "Revisado", formal: "Formal (e-mail)", whatsapp: "WhatsApp", ingles: "Inglês (traduzido)" };
/** "ingles" só vem do atalho de traduzir. */
export const MODOS = Object.keys(NOME_MODO).filter((m) => m !== "ingles");
export const NOMES_MOD: Record<string, string> = { ctrl: "Ctrl", alt: "Alt", shift: "Shift", win: "Win" };

export const pad = (n: number) => String(n).padStart(2, "0");
const chaveDia = (d: Date) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
/** ISO local sem fuso: o JS interpreta como hora local. */
export const dataDe = (iso: string) => new Date(iso);
export function nomeDia(iso: string) {
  const d = dataDe(iso), hoje = new Date(), ontem = new Date(hoje.getTime() - 864e5);
  return chaveDia(d) === chaveDia(hoje) ? "Hoje" : chaveDia(d) === chaveDia(ontem) ? "Ontem" : `${pad(d.getDate())}/${pad(d.getMonth() + 1)}/${d.getFullYear()}`;
}
export const hora = (iso: string) => { const d = dataDe(iso); return `${pad(d.getHours())}:${pad(d.getMinutes())}`; };
export function duracaoTxt(s: number) {
  const m = Math.round(s / 60);
  return s < 60 ? Math.round(s) + " s" : m < 60 ? m + " min" : Math.floor(m / 60) + " h " + pad(m % 60) + " min";
}
export const rotuloTecla = (p: string) => NOMES_MOD[p] || (p === "space" ? "Espaço" : p.toUpperCase());
export const rotuloAtalho = (a: string) => a.split("+").map(rotuloTecla).join(" + ");

const semPont = (w: string) => w.replace(/^[^\p{L}\p{N}]+|[^\p{L}\p{N}]+$/gu, "");
/** Diff simples por palavras: tira o começo e o fim iguais (sem pontuação, com maiúsculas) e o miolo é a troca. */
export function trocaDe(antes: string, depois: string): { de: string; para: string } | null {
  const a = antes.split(/\s+/).filter(Boolean), b = depois.split(/\s+/).filter(Boolean), eq = (x: string, y: string) => semPont(x) === semPont(y);
  let i = 0; while (i < a.length && i < b.length && eq(a[i], b[i])) i++;
  let j = 0; while (j < a.length - i && j < b.length - i && eq(a[a.length - 1 - j], b[b.length - 1 - j])) j++;
  const miolo = (p: string[]) => p.map(semPont).filter(Boolean), de = miolo(a.slice(i, a.length - j)), para = miolo(b.slice(i, b.length - j));
  return !para.length || para.length > 4 || de.length > 6 ? null : { de: de.join(" "), para: para.join(" ") };
}

export async function copiar(texto: string) {
  try { await navigator.clipboard.writeText(texto); }
  catch { const t = document.createElement("textarea"); t.value = texto; document.body.append(t); t.select(); document.execCommand("copy"); t.remove(); }
}

/** O "✓ Salvo" que acende por 1,4 s. */
export function useSalvo(): [boolean, () => void] {
  const [on, setOn] = useState(false);
  const t = useRef<number>(0);
  const acender = useCallback(() => {
    setOn(true); clearTimeout(t.current); t.current = window.setTimeout(() => setOn(false), 1400);
  }, []);
  return [on, acender];
}

/** Mesmo timbre do bipe do app: seno 880 Hz, 60 ms, envelope de 8 ms. */
let ctxAudio: AudioContext | undefined;
export function bipePrevia(v: number) {
  if (v <= 0) return;
  ctxAudio = ctxAudio || new AudioContext();
  const t = ctxAudio.currentTime, o = ctxAudio.createOscillator(), g = ctxAudio.createGain();
  o.frequency.value = 880; g.gain.setValueAtTime(0, t); g.gain.linearRampToValueAtTime(v / 100, t + 0.008);
  g.gain.setValueAtTime(v / 100, t + 0.052); g.gain.linearRampToValueAtTime(0, t + 0.06);
  o.connect(g).connect(ctxAudio.destination); o.start(t); o.stop(t + 0.062);
}

/** Junta várias mudanças seguidas numa gravação só, `atraso` ms depois da última. */
export function useLote(atraso: number, enviar: (v: Record<string, any>) => void) {
  const pend = useRef<Record<string, any>>({});
  const t = useRef(0);
  const f = useRef(enviar);
  f.current = enviar;
  return useCallback((v: Record<string, any>) => {
    pend.current = { ...pend.current, ...v };
    clearTimeout(t.current);
    t.current = window.setTimeout(() => { const e = pend.current; pend.current = {}; f.current(e); }, atraso);
  }, [atraso]);
}
