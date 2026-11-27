import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { Aparencia } from "./Aparencia";
import { chamar, emTauri } from "./api";
import { Chave, Sobre } from "./ChaveSobre";
import { Escrita } from "./Escrita";
import { Historico } from "./Historico";
import { Inicio } from "./Inicio";
import { Som } from "./Som";
import { Substituicoes } from "./Substituicoes";
import { Vocabulario } from "./Vocabulario";
import type { Ajustes, Dispositivo, Estado, Resposta } from "./types";
import { rotuloTecla, useSalvo } from "./util";
import versao from "./version.json";

const ic = (d: ReactNode) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8">{d}</svg>;
const ABAS: [string, string, ReactNode][] = [
  ["inicio", "Início", ic(<path d="M4 11l8-7 8 7M6 10v9h12v-9" />)],
  ["hist", "Histórico", ic(<><circle cx="12" cy="12" r="9" /><path d="M12 7v5l3 2" /></>)],
  ["voc", "Vocabulário", ic(<path d="M4 6h16M4 12h10M4 18h7" />)],
  ["sub", "Substituições", ic(<path d="M4 8h13l-3-3M20 16H7l3 3" />)],
  ["esc", "Escrita com IA", ic(<path d="M4 20h4L19 9l-4-4L4 16v4zM13 7l4 4" />)],
  ["som", "Ditado e som", ic(<path d="M4 9v6h4l5 4V5L8 9H4zM17 8.5a5 5 0 0 1 0 7" />)],
  ["apar", "Aparência", ic(<><circle cx="12" cy="12" r="4" /><path d="M12 3v2M12 19v2M3 12h2M19 12h2M5.6 5.6l1.4 1.4M17 17l1.4 1.4M5.6 18.4L7 17M17 7l1.4-1.4" /></>)],
  ["key", "Chave Groq", ic(<><circle cx="8" cy="15" r="4" /><path d="M11 12l9-9M17 6l3 3" /></>)],
  ["sob", "Sobre", ic(<><circle cx="12" cy="12" r="9" /><path d="M12 11v6M12 7.5v.5" /></>)],
];
const ALIAS: Record<string, string> = {
  inicio: "inicio", historico: "hist", hist: "hist", vocabulario: "voc", voc: "voc", substituicoes: "sub", sub: "sub",
  escrita: "esc", esc: "esc", ditado: "som", som: "som", aparencia: "apar", apar: "apar", chave: "key", groq: "key", key: "key", sobre: "sob", sob: "sob",
};
const daUrl = () => ALIAS[new URLSearchParams(location.search).get("aba") ?? ""] ?? "inicio";

const Logo = () => (
  <svg width="34" height="34" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64">
    <defs>
      <linearGradient id="fundo" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stopColor="#1e3a8a" /><stop offset="1" stopColor="#0b1530" /></linearGradient>
      <linearGradient id="luz" x1="0" x2="1"><stop offset="0" stopColor="#3b82f6" stopOpacity="0" /><stop offset=".5" stopColor="#dbeafe" /><stop offset="1" stopColor="#3b82f6" stopOpacity="0" /></linearGradient>
    </defs>
    <rect width="64" height="64" rx="15" fill="url(#fundo)" />
    <rect x="24" y="9" width="16" height="28" rx="8" fill="#fff" />
    <path d="M16.5 29.5a15.5 15.5 0 0 0 31 0" fill="none" stroke="#fff" strokeWidth="5" strokeLinecap="round" />
    <rect x="29.5" y="44" width="5" height="6" fill="#fff" />
    <rect x="6" y="52" width="52" height="5.5" rx="2.75" fill="url(#luz)" />
  </svg>
);

export default function App() {
  const [aba, setAba] = useState(daUrl);
  const [aj, setAj] = useState<Ajustes | null>(null);
  const [dispositivos, setDispositivos] = useState<Dispositivo[]>([]);
  const [estado, setEstado] = useState<Estado | null>(null);
  const [vocab, setVocab] = useState<string[]>([]);
  const [salvoVoc, acenderVoc] = useSalvo();
  // Abas que dependem de dados: o botão só aparece depois da inicialização dar certo.
  const [prontas, setProntas] = useState<Record<string, boolean>>({});
  const [baseOk, setBaseOk] = useState(false);
  const mostrado = useRef(false);

  const carregarAj = useCallback(async () => {
    const r = await chamar<{ ajustes?: Ajustes; dispositivos?: Dispositivo[] }>("ajustes");
    if (!r.ajustes) throw new Error("sem ajustes");
    setAj(r.ajustes); setDispositivos(r.dispositivos ?? []);
  }, []);
  const carregarEstado = useCallback(async () => {
    const e = await chamar<Estado>("estado");
    setEstado(e); setVocab(e.vocab);
  }, []);
  useEffect(() => {
    Promise.all([carregarEstado().catch(() => {}), carregarAj().catch(() => {})]).then(() => setBaseOk(true));
  }, [carregarAj, carregarEstado]);

  // O tema vale antes de a janela aparecer.
  useEffect(() => { document.documentElement.dataset.theme = aj?.tema ?? "sistema"; }, [aj?.tema]);

  // A aba aberta fica na URL: ao atualizar, a casca lê dali onde reabrir a janela (#14).
  useEffect(() => { history.replaceState(null, "", "?aba=" + aba); }, [aba]);

  // Aba pedida pela casca com a janela já aberta (tarefa 7 do menu da bandeja).
  useEffect(() => {
    if (!emTauri) return;
    const off = listen<string>("aba", (e) => setAba(ALIAS[e.payload] ?? "inicio"));
    return () => { void off.then((f) => f()); };
  }, []);
  useEffect(() => {
    const f = (e: KeyboardEvent) => { if (e.key === "Escape") emTauri ? void chamar("fechar") : window.close(); };
    addEventListener("keydown", f);
    return () => removeEventListener("keydown", f);
  }, []);

  const avisar = useCallback((nome: string, ok: boolean) => setProntas((p) => (p[nome] === ok ? p : { ...p, [nome]: ok })), []);
  const avisarInicio = useCallback((ok: boolean) => avisar("inicio", ok), [avisar]);
  const avisarHist = useCallback((ok: boolean) => avisar("hist", ok), [avisar]);
  const avisarSub = useCallback((ok: boolean) => avisar("sub", ok), [avisar]);

  // A janela nasce escondida e só aparece desenhada, sem piscar branco.
  useEffect(() => {
    if (mostrado.current || !baseOk || !("inicio" in prontas && "hist" in prontas && "sub" in prontas)) return;
    mostrado.current = true; void chamar("mostrar");
  }, [baseOk, prontas]);

  const mudarAj = useCallback((v: Partial<Ajustes>) => setAj((a) => a && ({
    ...a, ...v, luz: { ...a.luz, ...v.luz }, comandos: { ...a.comandos, ...v.comandos },
  })), []);
  /** Aplica na hora, grava e, se a gravação for recusada, volta ao que está no arquivo. */
  const salvarAj = useCallback(async (valores: Record<string, unknown>) => {
    mudarAj(valores as Partial<Ajustes>);
    const r = await chamar<Resposta>("salvar_ajustes", { valores });
    if (!r.ok) await carregarAj().catch(() => {});
    return r;
  }, [mudarAj, carregarAj]);

  const timerVoc = useRef(0);
  const alterarVocab = useCallback((termos: string[]) => {
    setVocab(termos);
    clearTimeout(timerVoc.current);
    timerVoc.current = window.setTimeout(async () => { await chamar("salvar_vocab", { termos }); acenderVoc(); }, 300);
  }, [acenderVoc]);

  const visivel = (a: string) => a === "inicio" || a === "hist" || a === "sub" ? prontas[a] === true : a === "esc" || a === "som" || a === "apar" ? aj !== null : true;
  const ativa = visivel(aba) || !baseOk ? aba : "voc";
  const abrir = (a: string) => { setAba(a); if (a === "sob") void carregarEstado().catch(() => {}); };
  const limite = estado?.limite ?? 850;
  const atalho = aj?.atalho_ditar ?? "ctrl+win";

  return (
    <>
      <aside>
        <div className="brand"><div className="logo"><Logo /></div><div><b>RenanTalk</b><small>v{versao.version}</small></div></div>
        <nav>
          {ABAS.map(([id, nome, icone]) => (
            <button key={id} className={ativa === id ? "on" : ""} hidden={!visivel(id)} onClick={() => abrir(id)}>{icone}{nome}</button>
          ))}
        </nav>
        <div className="hint">Segure {atalho.split("+").map((p, i) => <span key={p}>{i > 0 && " + "}<kbd>{rotuloTecla(p)}</kbd></span>)}, fale e solte.</div>
        <div className="glow" />
      </aside>
      <main>
        <Inicio ativa={ativa === "inicio"} atalhoColar={aj?.atalho_colar ?? ""} limite={limite} aoCarregar={avisarInicio} aoVocab={setVocab} />
        <Historico ativa={ativa === "hist"} vocab={vocab} aoCarregar={avisarHist} aoAdicionar={(t) => alterarVocab([...vocab, t])} />
        <Vocabulario ativa={ativa === "voc"} vocab={vocab} limite={limite} salvo={salvoVoc} aoAlterar={alterarVocab} />
        <Substituicoes ativa={ativa === "sub"} aj={aj} salvarAj={salvarAj} aoCarregar={avisarSub} />
        {aj && <>
          <Escrita ativa={ativa === "esc"} aj={aj} salvarAj={salvarAj} />
          <Som ativa={ativa === "som"} aj={aj} dispositivos={dispositivos} mudarAj={mudarAj} salvarAj={salvarAj} />
          <Aparencia ativa={ativa === "apar"} aj={aj} mudarAj={mudarAj} salvarAj={salvarAj} />
        </>}
        <Chave ativa={ativa === "key"} estado={estado} aoSalvar={(c) => setEstado((e) => e && { ...e, chave: c })} />
        <Sobre ativa={ativa === "sob"} estado={estado} />
      </main>
      <div className="versao">v{versao.version}{versao.builtAt ? " · " + versao.builtAt : ""}</div>
    </>
  );
}
