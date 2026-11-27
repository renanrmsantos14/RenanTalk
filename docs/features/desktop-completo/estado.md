# RenanTalk desktop mais completo (histórico, termos sugeridos, volume do bipe)

- Nível: 2 (várias telas, armazenamento novo, pedido aberto)
- Etapa: 5 de 7 Construir
- Próximo passo: tarefas 3 e 4 (colar último; aparência) · sozinho
- Branch e PR: t3/desktop-historico-termos-volume · ainda não
- Protótipo: B + C (abre na tela Início com resumo; Histórico em lista com detalhe e correção), escolhido em 26/11/2026
- Go: 26/11/2026 22:08 por Renan (clique no card)
- PROD: ainda não
- Atualizado: 26/11/2026 22:08

## Contexto levantado (código)
- Python (bandeja, ditado, luz) + janela de configurações em Tauri/Rust (`config/src/main.rs`, `app/ui/index.html`), com fallback Python+Edge (`app/configuracoes.py`).
- Hoje nada é gravado: o README promete "Nada é gravado em disco" e o log não guarda conteúdo. Histórico muda essa promessa.
- Bipe no Windows usa `winsound.Beep` (sem controle de volume); no Linux, `sounddevice` com amplitude fixa 0.18. Volume exige trocar o Windows para tom gerado.
- Vocabulário: `vocabulario.txt`, limite ~850 caracteres no prompt do Whisper.

## Decisões
| Data | Decisão | Por quê | Quem decidiu |
|---|---|---|---|
| 26/11/2026 | Escopo: histórico, termos sugeridos, volume do bipe + re-colar/copiar do histórico, estatísticas, escolher microfone, substituições de texto | pedido de app "mais completo" | Renan |
| 26/11/2026 | Histórico local guardado por 30 dias, com botão "apagar tudo" | equilíbrio entre utilidade e privacidade | Renan |
| 26/11/2026 | Sugestão de termos local: aprende com a correção feita no histórico e com a frequência de nomes e siglas; aceitar ou ignorar | sem custo e sem reenviar texto à Groq; pega erro do Whisper | Renan |
| 26/11/2026 | Som: volume de 0 a 100% (0 = mudo), com prévia, igual para os 3 bipes | pedido direto | Renan |
| 26/11/2026 | Re-colar: item no menu da bandeja + atalho de teclado | dois caminhos, o atalho é o mais rápido | Renan |
| 26/11/2026 | Substituições: comandos falados (nova linha, ponto final…) + atalhos próprios "quando eu disser → escreva" | os dois usos | Renan |
| 26/11/2026 | Entrega em 2 PRs: PR 1 histórico, re-colar, volume, microfone; PR 2 termos sugeridos, substituições, estatísticas | PR 2 depende do histórico; PR 1 já é usável | Renan |
| 26/11/2026 | Entram também: idioma (pt fixo, automático, inglês), atalho configurável, luz configurável (cor, espessura, posição, desligada), tema da janela, notificações liga/desliga | segunda rodada de ideias | Renan |
| 26/11/2026 | IA no texto (modelo de texto da Groq): modos de escrita (literal, revisado, formal/e-mail, WhatsApp), modo automático por aplicativo ativo, traduzir para inglês | segunda rodada de ideias; aceita reenviar o texto à Groq | Renan |
| 26/11/2026 | Fora: mãos livres, pausar música, ajustes de colagem, exportar/importar, sincronizar máquinas, pausar histórico, Whisper offline | não escolhidos | Renan |
| 26/11/2026 | Assumi: 3 PRs. PR 1 base (histórico, re-colar, volume, microfone, idioma, atalho, luz, tema, notificações); PR 2 inteligência local (termos, substituições, estatísticas); PR 3 IA no texto | o escopo cresceu; a IA é independente e tem custo próprio | Claude (assumido) |

## Tarefas e provas
| # | Tarefa | Depende de | Cenário (ação → resultado) | Prova | Nível | Resultado |
|---|---|---|---|---|---|---|
| 1 | PR 1 · `ajustes.json` (Python e Rust) + aba "Ditado e som": volume com bipe gerado no Windows, microfone, idioma, atalho configurável · Sonnet | — | 5, 6, 7, 8 | Linux: app rodando, bipes medidos, print da aba; Windows: notebook via delegação | 4 | passou (local): 13 testes; listener X real trocou ctrl+alt/ctrl+win sem reiniciar; clique real na janela Tauri (Xvfb) gravou atalho ctrl+alt. Falta: Windows | 
| 2 | PR 1 · `historico.db` + aba Histórico (layout C): gravar, buscar, editar, copiar, colar, excluir, retenção · Sonnet | 1 | 1, 2, 3, 10 | ditar de verdade e print do item; consulta SQLite antes e depois | 4 | passou (local): 22 testes; na janela Tauri real (Xvfb) editar gravou editado=1 com original preservado, excluir em 2 passos apagou a linha; retenção tirou o item de 40 dias (print). Corrigido: botão Confirmar exclusão ilegível no hover. Falta: ditado real gravando e Windows; Colar fica para a tarefa 3 |
| 3 | PR 1 · Colar último: menu da bandeja (Windows e Linux) + atalho · Sonnet | 1, 2 | 4 | atalho num editor de texto, print | — | pendente |
| 4 | PR 1 · Aparência: luz, tema, notificações · Sonnet | 1 | 9 | print da luz em cada posição e do tema | — | pendente |
| 5 | PR 2 · Tela Início (layout B) + estatísticas · Sonnet | 2 | 11 | print com dados reais do banco | — | pendente |
| 6 | PR 2 · Sugestões de termos (correção + frequência), aceitar/ignorar · Sonnet | 2, 5 | 12, 13 | teste com histórico de exemplo + print | — | pendente |
| 7 | PR 2 · Substituições (comandos + atalhos próprios) · Sonnet | 2 | 14 | teste unitário da função + ditado real | — | pendente |
| 8 | PR 3 · IA no texto: modos, modo por aplicativo, traduzir, fallback literal · Sonnet | 1, 2 | 15, 16, 17, 18 | ditado real em cada modo + teste de timeout | — | pendente |
| 9 | Cada PR · README, `verificar-ditado`/`verificar-janela`, versão na janela | cada PR | — | `verificacao-manter` | — | pendente |

## Ações críticas (só nível 3)
| # | Ação | Alvo real (ambiente, tabela, registros) | Simulação | Backup conferido | Plano de volta | Executada em |
|---|---|---|---|---|---|---|

## Pendências e fora de escopo
- Provar por clique na janela Tauri as abas novas (tela estava bloqueada em 26/11 22:18).
- Provar no Windows: bipe gerado, atalho via `keyboard` + Casador.
- Microfone plugado com o app aberto só aparece após reiniciar (cache do PortAudio).
- Prova de janela: Xvfb em :99 + xdotool (não usar a tela real). Disco raiz em 95% (scratchpads de outras sessões).
- App aparece como classe/exe (`code`, `outlook`), sem nome amigável.
- Rótulo "Segure Ctrl + Super" no menu da bandeja Linux ainda fixo.

## Retro
-
