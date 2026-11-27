import { Msg, Salvo } from "./Pecas";
import type { Ajustes, Luz, Resposta } from "./types";
import { useLote, useSalvo } from "./util";
import { useState } from "react";

const CORES: Record<string, [string, string, string]> = {
  azul: ["120,170,255", "37,99,235", "#2563eb"], verde: ["134,239,172", "22,163,74", "#16a34a"],
  roxo: ["196,167,255", "124,58,237", "#7c3aed"], ambar: ["253,224,130", "217,119,6", "#d97706"],
};
const PX: Record<string, number> = { fina: 2, media: 4, grossa: 8 };

function Grupo<T extends string>({ valor, opcoes, aoMudar, id, cores }: { valor: T; opcoes: [T, string][]; aoMudar: (v: T) => void; id: string; cores?: boolean }) {
  return (
    <div className={"mods" + (cores ? " sw" : "")} id={id} style={id === "g-tema" ? { margin: 0 } : undefined}>
      {opcoes.map(([v, r]) => (
        <button key={v} className={"mod" + (valor === v ? " on" : "")} onClick={() => aoMudar(v)}>
          {cores && <i style={{ background: CORES[v][2] }} />}{r}
        </button>
      ))}
    </div>
  );
}

export function Aparencia({ ativa, aj, mudarAj, salvarAj }: {
  ativa: boolean; aj: Ajustes; mudarAj: (v: Partial<Ajustes>) => void; salvarAj: (v: Record<string, unknown>) => Promise<Resposta>;
}) {
  const [salvo, acender] = useSalvo();
  const [msg, setMsg] = useState("");
  const lote = useLote(200, async (v) => {
    const r = await salvarAj(v);
    if (r.ok) { setMsg(""); acender(); } else setMsg(r.erro || "Não consegui salvar.");
  });
  const mudar = (v: Partial<Ajustes>) => { mudarAj(v); lote(v); };
  const luz = aj.luz, [c, p, hex] = CORES[luz.cor] ?? CORES.azul, px = PX[luz.espessura] ?? 4, topo = luz.posicao === "topo";
  const trocarLuz = (v: Partial<Luz>) => mudar({ luz: { ...luz, ...v } });
  return (
    <section className={"pane" + (ativa ? " on" : "")} id="apar">
      <h1>Aparência</h1>
      <p className="sub">A luz vale no próximo ditado; o tema muda na hora. Salva sozinho.</p>
      <div className="card"><div className="row"><div><div>Tema da janela <Salvo on={salvo} /></div><div className="k">Sistema segue o computador.</div></div>
        <Grupo id="g-tema" valor={aj.tema} opcoes={[["sistema", "Sistema"], ["claro", "Claro"], ["escuro", "Escuro"]]} aoMudar={(v) => mudar({ tema: v })} /></div></div>
      <div className="card"><label className="chk"><input type="checkbox" checked={aj.notificacoes} onChange={(e) => mudar({ notificacoes: e.target.checked })} /><span><div>Notificações da bandeja</div><div className="k">Avisos de atualização. Erros continuam no bipe grave e no log.</div></span></label></div>
      <div className="card"><label className="chk"><input type="checkbox" checked={luz.ligada} onChange={(e) => trocarLuz({ ligada: e.target.checked })} /><span><div>Luz do ditado</div><div className="k">A linha de luz na borda da tela enquanto você fala. Desligada, só o bipe avisa.</div></span></label>
        <div className="grp"><span className="k">Cor</span><Grupo id="g-cor" cores valor={luz.cor} opcoes={[["azul", "Azul"], ["verde", "Verde"], ["roxo", "Roxo"], ["ambar", "Âmbar"]]} aoMudar={(v) => trocarLuz({ cor: v })} /></div>
        <div className="grp"><span className="k">Espessura</span><Grupo id="g-esp" valor={luz.espessura} opcoes={[["fina", "Fina"], ["media", "Média"], ["grossa", "Grossa"]]} aoMudar={(v) => trocarLuz({ espessura: v })} /></div>
        <div className="grp"><span className="k">Posição</span><Grupo id="g-pos" valor={luz.posicao} opcoes={[["embaixo", "Embaixo"], ["topo", "Topo"]]} aoMudar={(v) => trocarLuz({ posicao: v })} /></div>
        <div className="previa" aria-label="Prévia da luz" style={{ opacity: luz.ligada ? 1 : 0.35 }}>
          <i style={{ height: px, top: topo ? 0 : "auto", bottom: topo ? "auto" : 0, background: `linear-gradient(90deg,rgba(${p},0),rgba(${c},1) 50%,rgba(${p},0))`, boxShadow: `0 ${topo ? "" : "-"}${px * 2}px ${px * 5}px ${px}px ${hex}88` }} />
        </div></div>
      <Msg texto={msg} erro />
    </section>
  );
}
