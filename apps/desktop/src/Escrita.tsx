import { useRef, useState } from "react";
import { Msg, Salvo } from "./Pecas";
import type { Ajustes, Resposta } from "./types";
import { MODOS, NOME_MODO, useSalvo } from "./util";

const Modos = ({ valor, aoMudar, rotulo }: { valor: string; aoMudar: (v: string) => void; rotulo: string }) => (
  <select className="sel" value={valor} aria-label={rotulo} onChange={(e) => aoMudar(e.target.value)}>
    {MODOS.map((m) => <option key={m} value={m}>{NOME_MODO[m]}</option>)}
  </select>
);

export function Escrita({ ativa, aj, salvarAj }: { ativa: boolean; aj: Ajustes; salvarAj: (v: Record<string, unknown>) => Promise<Resposta> }) {
  const [salvo, acender] = useSalvo();
  const [msg, setMsg] = useState("");
  const [app, setApp] = useState(""), [modoNovo, setModoNovo] = useState("literal");
  const campo = useRef<HTMLInputElement>(null);
  // salvarAj já aplica o valor na hora e, se a gravação for recusada, recarrega o que está no arquivo
  const gravar = async (v: Record<string, unknown>) => {
    const r = await salvarAj(v);
    if (r.ok) { acender(); setMsg(""); } else setMsg(r.erro || "Não consegui salvar.");
  };
  const nomes = Object.keys(aj.modos_apps).sort();
  const adicionar = () => {
    const n = app.trim().toLowerCase();
    if (!n) return setMsg("Escreva o nome do aplicativo.");
    setApp(""); void gravar({ modos_apps: { ...aj.modos_apps, [n]: modoNovo } }); campo.current?.focus();
  };
  return (
    <section className={"pane" + (ativa ? " on" : "")} id="esc">
      <h1>Escrita com IA</h1>
      <p className="sub">Reescreve o ditado com a IA da Groq (mesma chave) antes de colar. Se a IA falhar ou passar de 4 s, cola o texto como você falou. Salva sozinho.</p>
      <div className="card"><div className="row"><div><div>Modo padrão <Salvo on={salvo} /></div><div className="k">Literal não envia o texto à IA.</div></div>
        <Modos valor={aj.modo} rotulo="Modo padrão" aoMudar={(v) => gravar({ modo: v })} /></div></div>
      <div className="card"><div>Modo por aplicativo</div><div className="k">Vale quando o nome do programa em uso contém o nome abaixo (ex.: “outlook”, “whatsapp”, “code”).</div>
        <div className="regras">
          {!nomes.length && <div className="k" style={{ padding: "8px 0" }}>Nenhum aplicativo: vale o modo padrão em todos.</div>}
          {nomes.map((n) => {
            const { [n]: _, ...resto } = aj.modos_apps;
            return (
              <div className="regra" key={n}>
                <span>{n}</span>
                <Modos valor={aj.modos_apps[n]} rotulo={"Modo de " + n} aoMudar={(v) => gravar({ modos_apps: { ...aj.modos_apps, [n]: v } })} />
                <button title="Remover" aria-label={"Remover " + n} onClick={() => gravar({ modos_apps: resto })}>✕</button>
              </div>
            );
          })}
        </div>
        <div className="add">
          <input ref={campo} value={app} onChange={(e) => setApp(e.target.value)} onKeyDown={(e) => e.key === "Enter" && adicionar()} placeholder="Aplicativo" maxLength={60} aria-label="Aplicativo" />
          <Modos valor={modoNovo} rotulo="Modo do aplicativo" aoMudar={setModoNovo} />
          <button className="btn" onClick={adicionar}>Adicionar</button>
        </div>
        <Msg texto={msg} erro />
      </div>
      <div className="card"><div>Traduzir para inglês</div><div className="k">Segure o atalho de traduzir (em Ditado e som), fale em português e solte: cola em inglês.</div></div>
    </section>
  );
}
