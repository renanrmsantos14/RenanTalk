import { useRef, useState } from "react";
import { Salvo } from "./Pecas";

export function Vocabulario({ ativa, vocab, limite, salvo, aoAlterar }: {
  ativa: boolean; vocab: string[]; limite: number; salvo: boolean; aoAlterar: (termos: string[]) => void;
}) {
  const [novo, setNovo] = useState("");
  const campo = useRef<HTMLInputElement>(null);
  const adicionar = () => {
    const v = novo.trim();
    if (v && !vocab.some((t) => t.toLowerCase() === v.toLowerCase())) aoAlterar([...vocab, v]);
    setNovo(""); campo.current?.focus();
  };
  let usado = 0;
  const n = vocab.join(", ").length;
  return (
    <section className={"pane" + (ativa ? " on" : "")} id="voc">
      <h1>Vocabulário</h1>
      <p className="sub">Nomes, siglas e termos para o ditado escrever certo. Salva sozinho.</p>
      <div className="add">
        <input ref={campo} value={novo} placeholder="Adicionar termo e apertar Enter" maxLength={80}
          onChange={(e) => setNovo(e.target.value)} onKeyDown={(e) => e.key === "Enter" && adicionar()} />
        <button className="btn" onClick={adicionar}>Adicionar</button>
      </div>
      <div className="chips">
        {vocab.length === 0 && <div className="vazio">Nenhum termo ainda.<br />Adicione nomes de clientes, siglas e produtos.</div>}
        {vocab.map((t, i) => {
          usado += (usado ? 2 : 0) + t.length;
          return (
            <span key={t} className={"chip" + (usado > limite ? " fora" : "")}>
              <span title={t}>{t}</span>
              <button title={"Remover " + t} onClick={() => aoAlterar(vocab.filter((_, j) => j !== i))}>✕</button>
            </span>
          );
        })}
      </div>
      <div className="foot">
        <span>{`${vocab.length} termos · ${n}/${limite}` + (n > limite ? " · os riscados ficam de fora" : "")}</span>
        <div className={"bar" + (n > limite ? " cheia" : "")}><i style={{ width: Math.min(100, (n / limite) * 100) + "%" }} /></div>
        <Salvo on={salvo} />
      </div>
    </section>
  );
}
