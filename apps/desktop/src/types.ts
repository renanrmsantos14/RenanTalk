// Formas que o núcleo Rust devolve (as mesmas da janela antiga).
export interface Luz { ligada: boolean; cor: string; espessura: string; posicao: string }
export interface Ajustes {
  volume: number; volume_outros: number; microfone: string; idioma: string;
  atalho_ditar: string; atalho_colar: string; atalho_traduzir: string;
  luz: Luz; tema: string; notificacoes: boolean; abaixar_outros: boolean;
  comandos: Record<string, boolean>; modo: string; modos_apps: Record<string, string>;
}
export interface Dispositivo { nome: string; padrao: boolean }
export interface Estado {
  vocab: string[]; limite: number; chave: string; memoria: number | null;
  ditados_hoje: number; verificacao: string;
}
export interface Ditado {
  id: number; criado_em: string; texto: string; app: string; modo: string; idioma: string;
  duracao_s: number; palavras: number; editado: boolean;
}
export interface Sugestao { termo: string; motivo: string; contagem: number }
export interface Resumo {
  ok: boolean; erro?: string; vazio: boolean; hoje: number; semana: number; poupado_s: number;
  dias: { dia: string; palavras: number }[];
  ultimo: { texto: string; criado_em: string; app: string } | null;
  sugestoes: Sugestao[];
}
export interface Regra { quando: string; escreva: string }
export interface Resposta { ok: boolean; erro?: string; [k: string]: unknown }
