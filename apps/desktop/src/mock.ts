// Dados de exemplo para desenvolver a tela fora do Tauri (`pnpm dev`, no navegador). Em memória: salvar, editar e
// excluir funcionam até recarregar a página. Devolve a mesma forma dos comandos de `src-tauri/src/tela.rs`.
import type { Ajustes, Ditado, Regra } from "./types";

const pad = (n: number) => String(n).padStart(2, "0");
const agora = new Date();
/** ISO local sem fuso, como o `historico.db`. */
const iso = (diasAtras: number, h: number, m: number) => {
  const d = new Date(agora.getFullYear(), agora.getMonth(), agora.getDate() - diasAtras, h, m, 12);
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}:12`;
};
const palavras = (t: string) => t.split(/\s+/).filter(Boolean).length;

const EXEMPLOS: [number, number, number, string, string, string, number][] = [
  [0, 9, 41, "Reunião com a diretoria às dez, confirmar a van executiva para o Santos Dumont.", "outlook", "formal", 6.2],
  [0, 10, 5, "Mandar o orçamento da van executiva para o cliente.", "code", "literal", 4.1],
  [0, 11, 20, "Ligar para o motorista do aeroporto e avisar do atraso do voo.", "whatsapp", "whatsapp", 5.0],
  [1, 15, 2, "Buscar o cliente no Santos Dumont às oito horas.", "whatsapp", "whatsapp", 3.8],
  [1, 16, 30, "Atualizar a planilha de abastecimento da semana.", "code", "literal", 3.3],
  [2, 8, 15, "Enviar a proposta revisada para a Betinhos Executive Service.", "outlook", "revisado", 4.7],
  [3, 14, 0, "Marcar a manutenção preventiva do Sprinter.", "whatsapp", "literal", 2.9],
  [5, 9, 10, "Conferir as faturas do cartão corporativo.", "code", "literal", 3.0],
];
let proximoId = 1;
let ditados: Ditado[] = EXEMPLOS.map(([d, h, m, texto, app, modo, dur]) => ({
  id: proximoId++, criado_em: iso(d, h, m), texto, app, modo, idioma: "pt", duracao_s: dur, palavras: palavras(texto), editado: false,
}));

let vocab = ["Betinhos", "Betinhos Executive Service", "Dataverse", "Power Apps", "Power Automate", "Power BI", "SharePoint", "Microsoft 365", "RD Station", "WebResource", "Planner", "Teams", "WhatsApp"];
let regras: Regra[] = [{ quando: "meu email", escreva: "contato@exemplo.com.br" }, { quando: "assinatura", escreva: "Atenciosamente,\\nRenan" }];
const sugestoesEstado: Record<string, string> = {};
let chave = "gsk_ABCDEFGHIJKLMNOPQRSTUVWX1234";

let ajustes: Ajustes = {
  volume: 60, volume_outros: 30, microfone: "", idioma: "pt", atalho_ditar: "ctrl+win", atalho_colar: "alt+win+v", atalho_traduzir: "shift+win",
  luz: { ligada: true, cor: "azul", espessura: "media", posicao: "embaixo" }, tema: "sistema", notificacoes: true, abaixar_outros: false,
  comandos: { nova_linha: true, ponto_final: true, virgula: true, interrogacao: true, abre_parenteses: true, fecha_parenteses: true },
  modo: "literal", modos_apps: { code: "literal", olk: "formal", outlook: "formal", whatsapp: "whatsapp" },
};
const DISPOSITIVOS = [{ nome: "Microfone interno", padrao: true }, { nome: "Headset USB", padrao: false }];
const MODOS = ["literal", "revisado", "formal", "whatsapp"];
const ATALHOS = ["atalho_ditar", "atalho_colar", "atalho_traduzir"];

const resumo = () => {
  const dia = (n: number) => { const d = new Date(agora.getFullYear(), agora.getMonth(), agora.getDate() - n); return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`; };
  const dias = [6, 5, 4, 3, 2, 1, 0].map((n) => ({ dia: dia(n), palavras: ditados.filter((x) => x.criado_em.startsWith(dia(n))).reduce((s, x) => s + x.palavras, 0) }));
  const ord = [...ditados].sort((a, b) => b.criado_em.localeCompare(a.criado_em) || b.id - a.id);
  const semana = dias.reduce((s, d) => s + d.palavras, 0), seg = ditados.reduce((s, x) => s + x.duracao_s, 0);
  const sugestoes = [{ termo: "Santos Dumont", motivo: "frequencia", contagem: 6 }].filter((s) => !sugestoesEstado[s.termo] && !vocab.includes(s.termo));
  return {
    ok: true, vazio: !ditados.length, hoje: dias[6].palavras, semana, poupado_s: Math.max(0, semana / 40 * 60 - seg),
    dias, ultimo: ord[0] ? { id: ord[0].id, criado_em: ord[0].criado_em, texto: ord[0].texto, app: ord[0].app } : null, sugestoes,
  };
};

const mascarar = (k: string) => (k.length > 8 ? k.slice(0, 4) + "•".repeat(10) + k.slice(-4) : "");
const ok = { ok: true };
const erro = (e: string) => ({ ok: false, erro: e });
const atraso = <T,>(v: T) => new Promise<T>((r) => setTimeout(() => r(v), 30));

export async function mockInvoke(cmd: string, a: Record<string, any>): Promise<any> {
  switch (cmd) {
    case "estado":
      return atraso({ vocab, limite: 850, chave: mascarar(chave), memoria: 12, ditados_hoje: ditados.filter((x) => x.criado_em.startsWith(iso(0, 0, 0).slice(0, 10))).length, verificacao: "" });
    case "ajustes": return atraso({ ajustes: structuredClone(ajustes), dispositivos: DISPOSITIVOS });
    case "salvar_ajustes": {
      const v = a.valores as Record<string, any>;
      if (v.idioma && !["pt", "auto", "en"].includes(v.idioma)) return erro("Idioma inválido.");
      if (v.modo && !MODOS.includes(v.modo)) return erro("Modo de escrita inválido.");
      let aviso: string | undefined;
      for (const k of ATALHOS) {
        if (!(k in v)) continue;
        const partes = String(v[k]).split("+");
        const outro = ATALHOS.filter((o) => o !== k).map((o) => (o in v ? v[o] : (ajustes as any)[o]) as string);
        if (partes.length < 2 || ["ctrl+c", "ctrl+v", "alt+shift"].includes(v[k])) { aviso ??= "Atalho inválido: use Ctrl, Alt ou Win com mais uma tecla, e não atalhos do sistema (Alt + Shift, Win + letra, Ctrl + V e parecidos)."; delete v[k]; continue; }
        const conf = outro.find((o) => partes.every((p) => o.split("+").includes(p)) || o.split("+").every((p) => partes.includes(p)));
        if (conf) { aviso ??= `Esse atalho se confunde com outro (${conf}). Escolha outro.`; delete v[k]; }
      }
      const { luz, comandos, ...resto } = v;
      ajustes = { ...ajustes, ...resto, luz: { ...ajustes.luz, ...luz }, comandos: { ...ajustes.comandos, ...comandos } };
      return atraso(aviso ? erro(aviso) : ok);
    }
    case "historico_listar": {
      const b = String(a.busca || "").toLowerCase();
      const itens = ditados.filter((x) => x.texto.toLowerCase().includes(b)).sort((p, q) => q.criado_em.localeCompare(p.criado_em) || q.id - p.id);
      return atraso({ ok: true, itens: structuredClone(itens) });
    }
    case "historico_editar": {
      const x = ditados.find((d) => d.id === a.id);
      if (!x || !String(a.texto).trim()) return erro("Texto vazio.");
      x.texto = a.texto; x.palavras = palavras(a.texto); x.editado = true; return atraso(ok);
    }
    case "historico_excluir": ditados = ditados.filter((d) => d.id !== a.id); return atraso(ok);
    case "historico_apagar_tudo": ditados = []; return atraso(ok);
    case "salvar_vocab": vocab = (a.termos as string[]).map((t) => t.trim()).filter(Boolean); return atraso(ok);
    case "inicio": return atraso(resumo());
    case "sugestao_resolver":
      sugestoesEstado[a.termo] = a.acao === "aceitar" ? "aceita" : "ignorada";
      if (a.acao === "aceitar" && !vocab.some((t) => t.toLowerCase() === String(a.termo).toLowerCase())) vocab = [...vocab, a.termo];
      return atraso({ ok: true, vocab });
    case "substituicoes_ler": return atraso({ ok: true, regras: structuredClone(regras) });
    case "substituicoes_salvar": regras = a.regras; return atraso(ok);
    case "salvar_chave": {
      const k = String(a.chave).trim();
      if (!k.startsWith("gsk_")) return erro("A chave da Groq começa com gsk_.");
      if (k.includes("falsa")) return erro("A Groq recusou essa chave.");
      chave = k; return atraso({ ok: true, chave: mascarar(k) });
    }
    case "abrir": case "mostrar": case "fechar": return atraso({ ok: true });
    default: throw new Error(`comando desconhecido: ${cmd}`);
  }
}
