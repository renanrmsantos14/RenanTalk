import { useCallback, useEffect, useRef, useState } from "react";
import { chamar } from "./api";
import { Msg } from "./Pecas";
import type { Ditado, Resposta } from "./types";
import { copiar, dataDe, hora, nomeDia, NOME_IDIOMA, NOME_MODO, pad, trocaDe } from "./util";

interface Troca { id: number; de: string; para: string; feito?: boolean }

export function Historico({ ativa, vocab, aoCarregar, aoAdicionar }: {
  ativa: boolean; vocab: string[]; aoCarregar: (ok: boolean) => void; aoAdicionar: (termo: string) => void;
}) {
  const [itens, setItens] = useState<Ditado[]>([]);
  const [sel, setSel] = useState<number | null>(null);
  const [busca, setBusca] = useState("");
  const [confirma, setConfirma] = useState(false);
  const [confirmaTudo, setConfirmaTudo] = useState(false);
  const [rascunho, setRascunho] = useState<Record<number, string>>({}); // sobrevive a busca e troca de aba
  const [troca, setTroca] = useState<Troca | null>(null);
  const [msg, setMsg] = useState({ t: "", erro: false });
  const [erroLista, setErroLista] = useState("");
  const buscaRef = useRef(busca);
  buscaRef.current = busca;
  const selRef = useRef(sel);
  selRef.current = sel;
  const detRef = useRef<HTMLDivElement>(null);

  const carregar = useCallback(async () => {
    const r = await chamar<{ ok: boolean; erro?: string; itens: Ditado[] }>("historico_listar", { busca: buscaRef.current });
    if (!r.ok) { setErroLista(r.erro || "Não consegui ler o histórico."); return false; }
    setErroLista(""); setItens(r.itens);
    const atual = selRef.current;
    let novo = atual !== null && !r.itens.some((x) => x.id === atual) ? null : atual;
    if (novo === null && r.itens.length) novo = r.itens[0].id;
    if (novo !== atual) setConfirma(false); // a confirmação vale só para o item em que foi pedida
    setSel(novo);
    return true;
  }, []);

  useEffect(() => { carregar().then(aoCarregar, () => aoCarregar(false)); }, []); // eslint-disable-line react-hooks/exhaustive-deps
  useEffect(() => { if (ativa) void carregar(); }, [ativa]); // eslint-disable-line react-hooks/exhaustive-deps
  const primeira = useRef(true);
  useEffect(() => {
    if (primeira.current) { primeira.current = false; return; }
    const t = setTimeout(() => { setSel(null); selRef.current = null; void carregar(); }, 200);
    return () => clearTimeout(t);
  }, [busca, carregar]);

  const aviso = (t: string, erro = false) => { setMsg({ t, erro }); if (!erro) setTimeout(() => setMsg((m) => (m.t === t ? { t: "", erro: false } : m)), 2500); };
  const it = itens.find((x) => x.id === sel);
  const texto = it ? rascunho[it.id] ?? it.texto : "";

  const salvar = async () => {
    if (!it) return;
    const r = await chamar<Resposta>("historico_editar", { id: it.id, texto });
    if (!r.ok) return aviso(r.erro || "Não consegui salvar.", true);
    const t = trocaDe(it.texto, texto);
    setTroca(t && { ...t, id: it.id });
    setRascunho(({ [it.id]: _, ...resto }) => resto);
    await carregar(); aviso("Correção salva.");
    requestAnimationFrame(() => { if (detRef.current) detRef.current.scrollTop = detRef.current.scrollHeight; });
  };
  const excluir = async () => {
    if (!it) return;
    if (!confirma) return setConfirma(true);
    const r = await chamar<Resposta>("historico_excluir", { id: it.id });
    setRascunho(({ [it.id]: _, ...resto }) => resto); setSel(null); selRef.current = null; setConfirma(false);
    await carregar(); if (!r.ok) aviso(r.erro || "Não consegui excluir.", true);
  };
  const apagarTudo = async () => {
    if (!confirmaTudo) { setConfirmaTudo(true); setTimeout(() => setConfirmaTudo(false), 4000); return; }
    setConfirmaTudo(false);
    await chamar("historico_apagar_tudo"); setRascunho({}); setSel(null); selRef.current = null; await carregar();
  };

  let dia = "";
  const noVoc = troca ? vocab.some((x) => x.toLowerCase() === troca.para.toLowerCase()) : false;
  return (
    <section className={"pane" + (ativa ? " on" : "")} id="hist">
      <div className="hhead">
        <div><h1>Histórico</h1><p className="sub">Seus ditados ficam só neste computador, por 30 dias. O áudio não é guardado.</p></div>
        <button className={"btn sec " + (confirmaTudo ? "conf" : "perigo")} onClick={apagarTudo}>{confirmaTudo ? "Confirmar: apagar todos" : "Apagar tudo"}</button>
      </div>
      <div className="hist">
        <div className="hl">
          <input id="hbusca" value={busca} onChange={(e) => setBusca(e.target.value)} placeholder="Buscar nos ditados" aria-label="Buscar nos ditados" />
          <div className="lista">
            {erroLista ? <div className="vazio">{erroLista}</div> : !itens.length
              ? <div className="vazio">{busca ? "Nenhum ditado com essa busca." : <>Nenhum ditado ainda.<br />Segure o atalho, fale e solte: ele aparece aqui.</>}</div>
              : itens.map((x) => {
                const nd = nomeDia(x.criado_em), cab = nd !== dia;
                dia = nd;
                return (
                  <div key={x.id} style={{ display: "contents" }}>
                    {cab && <div className="dia">{nd}</div>}
                    <button className={"it" + (sel === x.id ? " on" : "")} onClick={() => { setSel(x.id); setConfirma(false); }}>
                      <small>{hora(x.criado_em) + (x.app ? " · " + x.app : "") + (x.editado ? " · editado" : "")}</small>
                      <span>{x.texto.split("\n")[0]}</span>
                    </button>
                  </div>
                );
              })}
          </div>
        </div>
        <div className="hd" ref={detRef}>
          {!it ? <div className="vazio">{itens.length ? "Escolha um ditado na lista." : "Quando você ditar, o texto aparece aqui para corrigir e copiar."}</div> : <>
            <div className="k">{(() => { const d = dataDe(it.criado_em); return `${nomeDia(it.criado_em)}, ${pad(d.getDate())}/${pad(d.getMonth() + 1)}/${d.getFullYear()} às ${hora(it.criado_em)}`; })()}</div>
            <div className="tags">
              {[it.app && "App: " + it.app, "Modo: " + (NOME_MODO[it.modo] || it.modo), it.idioma && "Idioma: " + (NOME_IDIOMA[it.idioma] || it.idioma),
                it.duracao_s ? "Duração: " + it.duracao_s.toFixed(1).replace(".", ",") + " s" : "", it.palavras + " palavras", it.editado && "Editado"]
                .filter(Boolean).map((t) => <span className="tag" key={t as string}>{t}</span>)}
            </div>
            <textarea value={texto} aria-label="Texto do ditado"
              onChange={(e) => { const v = e.target.value; setRascunho((r) => { const { [it.id]: _, ...resto } = r; return v === it.texto ? resto : { ...r, [it.id]: v }; }); }} />
            <div className="acoes">
              <button className="btn" disabled={texto === it.texto || !texto.trim()} onClick={salvar}>Salvar correção</button>
              <button className="btn sec" onClick={async () => { await copiar(texto); aviso("Copiado."); }}>Copiar</button>
              <button className={"btn sec " + (confirma ? "conf" : "perigo")} onClick={excluir}>{confirma ? "Confirmar exclusão" : "Excluir"}</button>
              <Msg texto={msg.t} erro={msg.erro} estilo={{ margin: 0, display: "inline" }} />
            </div>
            {troca && troca.id === it.id && (
              <div className="troca">
                <div>
                  {troca.de ? `Troca: “${troca.de}” → “${troca.para}”` : `Você acrescentou “${troca.para}”`}
                  {troca.feito ? " · adicionado ao vocabulário." : noVoc ? " · já está no vocabulário." : ""}
                </div>
                <div className="acoes">
                  {!troca.feito && !noVoc && <button className="btn peq" onClick={() => { aoAdicionar(troca.para); setTroca({ ...troca, feito: true }); }}>Adicionar ao vocabulário</button>}
                  <button className="btn sec peq" onClick={() => setTroca(null)}>Dispensar</button>
                </div>
              </div>
            )}
          </>}
        </div>
      </div>
    </section>
  );
}
