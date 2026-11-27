import { useState } from "react";
import { Slider, Salvo } from "./Pecas";
import type { Ajustes, Dispositivo, Resposta } from "./types";
import { NOMES_MOD, bipePrevia, rotuloAtalho, useLote, useSalvo } from "./util";

const TECLAS: [string, string][] = [
  ...[..."ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"].map((c): [string, string] => [c.toLowerCase(), c]),
  ...Array.from({ length: 12 }, (_, i): [string, string] => ["f" + (i + 1), "F" + (i + 1)]), ["space", "Espaço"],
];
const ORDEM = ["ctrl", "alt", "shift", "win"];

/** Um cartão de atalho (Ctrl/Alt/Shift/Win + tecla opcional). Remonta quando a gravação é recusada. */
function CartaoAtalho({ titulo, dica, rotulo, valor, erro, aoMudar }: {
  titulo: string; dica: string; rotulo: string; valor: string; erro: string; aoMudar: (v: string) => void;
}) {
  const partes = valor.split("+");
  const [mods, setMods] = useState(new Set(partes.filter((p) => NOMES_MOD[p])));
  const [tecla, setTecla] = useState(partes.find((p) => !NOMES_MOD[p]) || "");
  const [local, setLocal] = useState("");
  const trocar = (m: Set<string>, t: string) => {
    const novo = [...ORDEM.filter((x) => m.has(x)), ...(t ? [t] : [])];
    if (novo.length < 2) return setLocal("O atalho precisa de pelo menos 2 teclas diferentes.");
    setLocal(""); aoMudar(novo.join("+"));
  };
  const atual = [...ORDEM.filter((x) => mods.has(x)), ...(tecla ? [tecla] : [])].join("+");
  return (
    <div className="card atalho">
      <div className="row"><div><div>{titulo}</div><div className="k">{dica}</div></div><kbd className="atalhov">{rotuloAtalho(atual)}</kbd></div>
      <div className="mods">
        {ORDEM.map((m) => (
          <button key={m} className={"mod" + (mods.has(m) ? " on" : "")} onClick={() => {
            const n = new Set(mods); n.has(m) ? n.delete(m) : n.add(m); setMods(n); trocar(n, tecla);
          }}>{NOMES_MOD[m]}</button>
        ))}
        <span className="k">+</span>
        <select className="sel tecla" value={tecla} aria-label={"Tecla extra do atalho de " + rotulo} onChange={(e) => { setTecla(e.target.value); trocar(mods, e.target.value); }}>
          <option value="">sem tecla extra</option>
          {TECLAS.map(([v, r]) => <option key={v} value={v}>{r}</option>)}
        </select>
      </div>
      <div className={"msg amsg" + (local || erro ? " err" : "")}>{local || erro}</div>
    </div>
  );
}

export function Som({ ativa, aj, dispositivos, mudarAj, salvarAj }: {
  ativa: boolean; aj: Ajustes; dispositivos: Dispositivo[];
  mudarAj: (v: Partial<Ajustes>) => void; salvarAj: (v: Record<string, unknown>) => Promise<Resposta>;
}) {
  const [salvo, acender] = useSalvo();
  const [erros, setErros] = useState<Record<string, string>>({});
  const [reinicio, setReinicio] = useState(0);
  const lote = useLote(250, async (v) => {
    const r = await salvarAj(v);
    if (r.ok) { acender(); setErros({}); return; }
    // o que foi recusado não vale: os cartões voltam ao que está gravado (salvarAj já recarregou)
    const chaves = Object.keys(v).filter((k) => k.startsWith("atalho_"));
    const e = r.erro || "Não consegui salvar os ajustes.";
    setErros(Object.fromEntries((chaves.length ? chaves : ["atalho_ditar"]).map((k) => [k, e])));
    setReinicio((n) => n + 1);
  });
  const mudar = (v: Record<string, any>) => { mudarAj(v); lote(v); };
  const [vol, setVol] = useState<number | null>(null), [volo, setVolo] = useState<number | null>(null);
  const v = vol ?? aj.volume, vo = volo ?? aj.volume_outros;
  const mic = aj.microfone;
  const tem = !mic || dispositivos.some((d) => d.nome === mic);
  const atalho = (chave: "atalho_ditar" | "atalho_colar" | "atalho_traduzir", titulo: string, dica: string, rotulo: string) => (
    <CartaoAtalho key={chave + reinicio} titulo={titulo} dica={dica} rotulo={rotulo} valor={aj[chave]} erro={erros[chave] || ""} aoMudar={(x) => mudar({ [chave]: x })} />
  );
  return (
    <section className={"pane" + (ativa ? " on" : "")} id="som">
      <h1>Ditado e som</h1>
      <p className="sub">Vale no próximo ditado, sem reiniciar. Salva sozinho.</p>
      <div className="card"><div className="row"><div><div>Volume dos bipes</div><div className="k">0% deixa tudo mudo.</div></div><Salvo on={salvo} /></div>
        <div className="vol">
          <Slider valor={v} min={0} max={100} passo={1} rotulo="Volume dos bipes" aoMover={setVol} aoSoltar={(x) => { setVol(null); bipePrevia(x); mudar({ volume: x }); }} />
          <b>{v}%</b><button className="btn sec" onClick={() => bipePrevia(v)}>Ouvir</button>
        </div></div>
      <div className="card"><label className="chk"><input type="checkbox" checked={aj.abaixar_outros} onChange={(e) => mudar({ abaixar_outros: e.target.checked })} /><span><div>Abaixar os outros sons enquanto fala</div><div className="k">Música, vídeos e chamadas baixam quando você segura o atalho e voltam quando solta.</div></span></label>
        <div className="vol"><span className="k">Ficam em</span>
          <Slider valor={vo} min={0} max={100} passo={5} rotulo="Volume dos outros apps durante o ditado" desligado={!aj.abaixar_outros} aoMover={setVolo} aoSoltar={(x) => { setVolo(null); mudar({ volume_outros: x }); }} />
          <b>{vo}%</b><span className="k">do volume de cada um</span></div></div>
      <div className="card"><div className="row"><div><div>Microfone</div><div className="k">A lista é atualizada quando a janela abre.</div></div>
        <select className="sel" value={mic} aria-label="Microfone" onChange={(e) => mudar({ microfone: e.target.value })}>
          <option value="">Padrão do sistema</option>
          {dispositivos.map((d) => <option key={d.nome} value={d.nome}>{d.nome + (d.padrao ? " (padrão)" : "")}</option>)}
          {!tem && <option value={mic}>{mic + " (não encontrado)"}</option>}
        </select></div></div>
      <div className="card"><div className="row"><div><div>Idioma do ditado</div><div className="k">Automático deixa o Whisper detectar.</div></div>
        <select className="sel" value={aj.idioma} aria-label="Idioma" onChange={(e) => mudar({ idioma: e.target.value })}>
          <option value="pt">Português</option><option value="auto">Automático</option><option value="en">Inglês</option>
        </select></div></div>
      {atalho("atalho_ditar", "Atalho de ditar", "Segure, fale e solte. Mínimo de 2 teclas.", "ditar")}
      {atalho("atalho_colar", "Atalho de colar o último ditado", "Cola de novo o texto do ditado mais recente. Também está no menu da bandeja.", "colar")}
      {atalho("atalho_traduzir", "Atalho de traduzir", "Segure, fale em português e solte: cola em inglês (usa a IA da Groq).", "traduzir")}
    </section>
  );
}
