import { useCallback, useEffect, useState } from "react";
import { chamar } from "./api";
import { Msg } from "./Pecas";
import type { Resposta, Resumo } from "./types";
import { copiar, duracaoTxt, hora, nomeDia, pad, rotuloAtalho } from "./util";

const DIA_SEMANA = ["dom", "seg", "ter", "qua", "qui", "sex", "sáb"];

export function Inicio({ ativa, atalhoColar, limite, aoCarregar, aoVocab }: {
  ativa: boolean; atalhoColar: string; limite: number;
  aoCarregar: (ok: boolean) => void; aoVocab: (termos: string[]) => void;
}) {
  const [r, setR] = useState<Resumo | null>(null);
  const [msg, setMsg] = useState<{ t: string; erro?: boolean }>({ t: "" });

  const carregar = useCallback(async () => {
    try {
      const x = await chamar<Resumo>("inicio");
      if (!x.ok) throw new Error(x.erro);
      setR(x); aoCarregar(true);
    } catch { aoCarregar(false); }
  }, [aoCarregar]);
  useEffect(() => { if (ativa || !r) void carregar(); }, [ativa]); // eslint-disable-line react-hooks/exhaustive-deps

  const aviso = (t: string, erro?: boolean) => { setMsg({ t, erro }); if (!erro) setTimeout(() => setMsg((m) => (m.t === t ? { t: "" } : m)), 2500); };
  const resolver = async (termo: string, acao: string) => {
    const x = await chamar<Resposta>("sugestao_resolver", { termo, acao });
    if (!x.ok) return aviso((x.erro as string) || "Não consegui salvar.", true);
    const novo = x.vocab as string[] | undefined;
    if (novo) aoVocab(novo);
    await carregar();
    if (novo && novo.join(", ").length > limite) aviso(`Vocabulário cheio: “${termo}” ficou de fora do ditado. Remova um termo em Vocabulário.`, true);
  };

  const max = Math.max(1, ...(r?.dias.map((d) => d.palavras) ?? []));
  const u = r?.ultimo ?? null;
  return (
    <section className={"pane" + (ativa ? " on" : "")} id="inicio">
      <h1>Início</h1>
      <p className="sub">Seus ditados dos últimos 7 dias. Tudo fica só neste computador.</p>
      {r?.vazio && <div className="card vazio">Nenhum ditado ainda.<br />Segure o atalho, fale e solte: o resumo, o último texto e as sugestões de termos aparecem aqui.</div>}
      {r && !r.vazio && <>
        <div className="stat">
          <div><span className="k">Palavras hoje</span><b>{r.hoje.toLocaleString("pt-BR")}</b></div>
          <div><span className="k">Palavras na semana</span><b>{r.semana.toLocaleString("pt-BR")}</b></div>
          <div><span className="k">Tempo poupado na semana</span><b>{duracaoTxt(r.poupado_s)}</b><span className="k">estimativa: digitando a 40 palavras por minuto</span></div>
        </div>
        <div className="card"><div className="k">Palavras por dia</div>
          <div className="grafico" role="img" aria-label="Palavras ditadas por dia, nos últimos 7 dias">
            {r.dias.map((d, i) => {
              const [y, m, dd] = d.dia.split("-").map(Number), hoje = i === r.dias.length - 1;
              return (
                <div key={d.dia} className={"col" + (hoje ? " hoje" : "")} title={`${pad(dd)}/${pad(m)}: ${d.palavras} palavras`}>
                  <b>{d.palavras || ""}</b>
                  <div className="bw"><i style={{ height: (d.palavras / max) * 100 + "%" }} /></div>
                  <small>{hoje ? "hoje" : DIA_SEMANA[new Date(y, m - 1, dd).getDay()]}</small>
                </div>
              );
            })}
          </div>
        </div>
        <div className="card">
          <div className="row">
            <div className="k">{u ? `Último ditado · ${nomeDia(u.criado_em)} às ${hora(u.criado_em)}` + (u.app ? " · " + u.app : "") : "Último ditado"}</div>
            <div className="acoes">{u && <button className="btn sec peq" onClick={async () => { await copiar(u.texto); aviso("Copiado."); }}>Copiar</button>}</div>
          </div>
          <div className="ultimo">{u?.texto}</div>
          <div className="k">{u && atalhoColar ? "Para colar de novo no programa em uso: " + rotuloAtalho(atalhoColar) + "." : ""}</div>
          <Msg texto={msg.t} erro={msg.erro} estilo={{ margin: "6px 0 0" }} />
        </div>
      </>}
      {r && r.sugestoes.length > 0 && (
        <div className="card"><div>Termos que você repete</div>
          <div className="k">Nomes e siglas ditados várias vezes. Aceitar leva o termo para o Vocabulário; ignorar não sugere de novo.</div>
          <div style={{ marginTop: 8 }}>
            {r.sugestoes.map((sg) => (
              <div className="item" key={sg.termo}>
                <div><b>{sg.termo}</b><div className="k">{sg.motivo === "correcao" ? "Você corrigiu para este termo" : `Apareceu ${sg.contagem} vezes nos últimos 30 dias`}</div></div>
                <button className="btn peq" onClick={() => resolver(sg.termo, "aceitar")}>Aceitar</button>
                <button className="btn peq sec" onClick={() => resolver(sg.termo, "ignorar")}>Ignorar</button>
              </div>
            ))}
          </div>
        </div>
      )}
      {r && r.vazio && msg.t && <Msg texto={msg.t} erro={msg.erro} />}
    </section>
  );
}
