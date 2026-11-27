import { useCallback, useEffect, useRef, useState } from "react";
import { chamar } from "./api";
import { Msg, Salvo } from "./Pecas";
import type { Ajustes, Regra, Resposta } from "./types";
import { useSalvo } from "./util";

const COMANDOS_UI: [string, string, string][] = [
  ["nova_linha", "Nova linha", "“nova linha” quebra a linha"], ["ponto_final", "Ponto final", "“ponto final” escreve ."],
  ["virgula", "Vírgula", "“vírgula” escreve ,"], ["interrogacao", "Ponto de interrogação", "“ponto de interrogação” escreve ?"],
  ["abre_parenteses", "Abre parênteses", "“abre parênteses” escreve ("], ["fecha_parenteses", "Fecha parênteses", "“fecha parênteses” escreve )"],
];

export function Substituicoes({ ativa, aj, salvarAj, aoCarregar }: {
  ativa: boolean; aj: Ajustes | null; salvarAj: (v: Record<string, unknown>) => Promise<Resposta>; aoCarregar: (ok: boolean) => void;
}) {
  const [regras, setRegras] = useState<Regra[]>([]);
  const [msg, setMsg] = useState({ t: "", erro: false });
  const [quando, setQuando] = useState(""), [escreva, setEscreva] = useState("");
  const [salvo, acenderSalvo] = useSalvo();
  const campo = useRef<HTMLInputElement>(null);

  const carregar = useCallback(async () => {
    const r = await chamar<{ ok: boolean; regras: Regra[] }>("substituicoes_ler");
    setRegras(r.regras);
  }, []);
  useEffect(() => { carregar().then(() => aoCarregar(true), () => aoCarregar(false)); }, []); // eslint-disable-line react-hooks/exhaustive-deps

  const gravar = async (novas: Regra[]) => {
    setRegras(novas);
    const r = await chamar<Resposta>("substituicoes_salvar", { regras: novas });
    if (r.ok) { setMsg({ t: "", erro: false }); acenderSalvo(); }
    else { setMsg({ t: r.erro || "Não consegui salvar.", erro: true }); await carregar(); }
  };
  const adicionar = () => {
    const q = quando.trim(), e = escreva.trim();
    if (!q || !e) return setMsg({ t: "Preencha “Quando eu disser” e “Escreva”.", erro: true });
    if (regras.some((r) => r.quando.toLowerCase() === q.toLowerCase())) return setMsg({ t: `Já existe uma regra para “${q}”.`, erro: true });
    setQuando(""); setEscreva(""); void gravar([...regras, { quando: q, escreva: e }]); campo.current?.focus();
  };
  const alternar = async (chave: string, on: boolean) => {
    const r = await salvarAj({ comandos: { [chave]: on } });
    if (r.ok) acenderSalvo(); else setMsg({ t: r.erro || "Não consegui salvar.", erro: true });
  };
  const enter = (e: React.KeyboardEvent) => e.key === "Enter" && adicionar();
  return (
    <section className={"pane" + (ativa ? " on" : "")} id="sub">
      <h1>Substituições</h1>
      <p className="sub">Troca o que você fala por outro texto, antes de colar. Vale no próximo ditado. Salva sozinho.</p>
      <div className="card"><div className="row"><div>Comandos falados</div><Salvo on={salvo} /></div>
        <div style={{ marginTop: 6 }}>
          {COMANDOS_UI.map(([chave, nome, dica]) => (
            <label className="chk item" key={chave}>
              <input type="checkbox" checked={aj?.comandos[chave] !== false} onChange={(e) => alternar(chave, e.target.checked)} />
              <span><div>{nome}</div><div className="k">{dica}</div></span>
            </label>
          ))}
        </div>
      </div>
      <div className="card"><div>Suas regras</div>
        <div className="k">Ex.: “meu email” vira o endereço completo. Use \n para quebrar a linha. Não diferencia maiúsculas e vale antes dos comandos.</div>
        <div className="regras">
          {regras.length === 0 && <div className="k" style={{ padding: "8px 0" }}>Nenhuma regra ainda.</div>}
          {regras.map((r, i) => (
            <div className="regra" key={r.quando}>
              <span>{r.quando}</span><span>→ {r.escreva}</span>
              <button title="Remover" aria-label={"Remover regra " + r.quando} onClick={() => gravar(regras.filter((_, j) => j !== i))}>✕</button>
            </div>
          ))}
        </div>
        <div className="add">
          <input ref={campo} value={quando} onChange={(e) => setQuando(e.target.value)} onKeyDown={enter} placeholder="Quando eu disser" maxLength={200} aria-label="Quando eu disser" />
          <input value={escreva} onChange={(e) => setEscreva(e.target.value)} onKeyDown={enter} placeholder="Escreva" maxLength={500} aria-label="Escreva" />
          <button className="btn" onClick={adicionar}>Adicionar</button>
        </div>
        <Msg texto={msg.t} erro={msg.erro} />
      </div>
    </section>
  );
}
