import { useEffect, useRef, useState } from "react";
import { chamar } from "./api";
import { Msg } from "./Pecas";
import type { Estado, Resposta } from "./types";
import version from "./version.json";

export function Chave({ ativa, estado, aoSalvar }: { ativa: boolean; estado: Estado | null; aoSalvar: (mascarada: string) => void }) {
  const [nova, setNova] = useState("");
  const [msg, setMsg] = useState({ t: "", cls: "" });
  const [ocupado, setOcupado] = useState(false);
  const campo = useRef<HTMLInputElement>(null);
  useEffect(() => { if (ativa) campo.current?.focus(); }, [ativa]);
  // Nada daqui vai a log nem a console: a chave só viaja no comando e a tela guarda só a versão mascarada.
  const salvar = async () => {
    const k = nova.trim();
    if (!k || ocupado) return;
    setOcupado(true); setMsg({ t: "Validando na Groq…", cls: "mut" });
    try {
      const r = await chamar<Resposta>("salvar_chave", { chave: k });
      if (r.ok) { setMsg({ t: "✓ Chave salva. Já está em uso.", cls: "ok" }); aoSalvar(r.chave as string); setNova(""); }
      else setMsg({ t: r.erro || "Não consegui validar a chave.", cls: "err" });
    } catch { setMsg({ t: "Não consegui validar a chave.", cls: "err" }); }
    setOcupado(false);
  };
  return (
    <section className={"pane" + (ativa ? " on" : "")} id="key">
      <h1>Chave Groq</h1>
      <p className="sub">Usada para transcrever. Fica só neste computador.</p>
      <div className="card"><div className="row"><div><div className="k">Chave atual</div><div className="mono">{estado ? estado.chave || "Nenhuma chave salva" : "—"}</div></div>
        {estado && (estado.chave ? <span><span style={{ color: "var(--ok)" }}>● Salva</span></span> : <span><span style={{ color: "var(--err)" }}>● Falta a chave</span></span>)}</div></div>
      <div className="add">
        <input ref={campo} type="password" value={nova} onChange={(e) => setNova(e.target.value)} onKeyDown={(e) => e.key === "Enter" && salvar()}
          placeholder="Colar nova chave (gsk_…)" autoComplete="off" />
        <button className="btn" disabled={ocupado} onClick={salvar}>Validar e salvar</button>
      </div>
      <Msg texto={msg.t} erro={msg.cls === "err"} mut={msg.cls === "mut"} />
      <p className="k">Não tem chave? <span className="links"><button onClick={() => chamar("abrir", { alvo: "groq" })}>Criar em console.groq.com</button></span></p>
    </section>
  );
}

const SITUACAO: Record<string, (v: string) => string> = {
  em_dia: (v) => `✓ Em dia (v${v}).`,
  instalando: (v) => `Instalando v${v}. O RenanTalk reinicia sozinho e reabre aqui.`,
  adiada: (v) => `v${v} disponível; instalo quando o ditado acabar.`,
};

export function Sobre({ ativa, estado }: { ativa: boolean; estado: Estado | null }) {
  const [msg, setMsg] = useState({ t: "", cls: "" });
  const [ocupado, setOcupado] = useState(false);
  const procurar = async () => {
    setOcupado(true); setMsg({ t: "Procurando atualização…", cls: "mut" });
    try {
      const r = await chamar<Resposta>("procurar_atualizacao");
      if (r.ok) setMsg({ t: SITUACAO[r.situacao as string]?.(r.versao as string) ?? "", cls: "ok" });
      else setMsg({ t: r.erro || "Não consegui procurar atualização.", cls: "err" });
      if (r.situacao === "instalando") return; // fica ocupado até o app reiniciar
    } catch { setMsg({ t: "Não consegui procurar atualização.", cls: "err" }); }
    setOcupado(false);
  };
  return (
    <section className={"pane" + (ativa ? " on" : "")} id="sob">
      <h1>Sobre</h1>
      <p className="sub">Atualiza sozinho pelos releases do GitHub.</p>
      <div className="stat">
        <div><span className="k">Versão</span><b>{version.version}</b></div>
        <div><span className="k">Memória</span><b>{estado?.memoria ? estado.memoria + " MB" : "—"}</b></div>
        <div><span className="k">Ditados hoje</span><b>{estado ? estado.ditados_hoje : "—"}</b></div>
      </div>
      <div className="card"><div className="row">
        <div><div>Atualização</div><div className="k">{estado?.verificacao ? "Última verificação: " + estado.verificacao : "Ainda não verificou hoje"}</div></div>
        <button className="btn sec" disabled={ocupado} onClick={procurar}>Procurar agora</button>
      </div><Msg texto={msg.t} erro={msg.cls === "err"} mut={msg.cls === "mut"} /></div>
      <div className="links"><button onClick={() => chamar("abrir", { alvo: "log" })}>Ver log</button><button onClick={() => chamar("abrir", { alvo: "repo" })}>Repositório no GitHub</button></div>
    </section>
  );
}
