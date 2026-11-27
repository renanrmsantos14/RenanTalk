# RenanTalk desktop mais completo (histórico, termos sugeridos, volume do bipe)

- Nível: 2 (várias telas, armazenamento novo, pedido aberto)
- Etapa: 7 de 7 Entregar (PR 2 e PR 3 mergeados em desktop-completo em 27/11; PR 1 #6 leva tudo à main)
- Próximo passo: Renan decide o merge do #8 e do #6 (merge na main publica release para todas as máquinas)
- Branch e PR: t3/desktop-historico-termos-volume (remota: desktop-completo) · PR 1: https://github.com/renanrmsantos14/RenanTalk/pull/6 (rascunho)
- Protótipo: B + C (abre na tela Início com resumo; Histórico em lista com detalhe e correção), escolhido em 26/11/2026
- Go: 26/11/2026 22:08 por Renan (clique no card)
- PROD: ainda não
- Atualizado: 27/11/2026 08:34
- PR do abaixar volume: #7 (merged). App que fecha durante o ditado: #8 (pronto, Windows provado).
- PR 2: branch desktop-inteligencia, base desktop-completo
- PR 3: branch desktop-ia, base desktop-inteligencia

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
| 26/11/2026 | Abaixar os outros sons enquanto fala: desligado por padrão; ligado, os outros apps vão a 30% do volume de cada um (0–100, passo 5) e voltam ao soltar; restauração sempre ao valor de antes (o Spotify reage à mudança externa). PR próprio empilhado sobre o PR 1 | pedido direto do Renan em 26/11 | Renan (pedido) + Claude (padrões assumidos) |
| 26/11/2026 | Assumi: 3 PRs. PR 1 base (histórico, re-colar, volume, microfone, idioma, atalho, luz, tema, notificações); PR 2 inteligência local (termos, substituições, estatísticas); PR 3 IA no texto | o escopo cresceu; a IA é independente e tem custo próprio | Claude (assumido) |

## Tarefas e provas
| # | Tarefa | Depende de | Cenário (ação → resultado) | Prova | Nível | Resultado |
|---|---|---|---|---|---|---|
| 1 | PR 1 · `ajustes.json` (Python e Rust) + aba "Ditado e som": volume com bipe gerado no Windows, microfone, idioma, atalho configurável · Sonnet | — | 5, 6, 7, 8 | Linux: app rodando, bipes medidos, print da aba; Windows: notebook via delegação | 4 | passou (local): 13 testes; listener X real trocou ctrl+alt/ctrl+win sem reiniciar; clique real na janela Tauri (Xvfb) gravou atalho ctrl+alt. Falta: Windows | 
| 2 | PR 1 · `historico.db` + aba Histórico (layout C): gravar, buscar, editar, copiar, colar, excluir, retenção · Sonnet | 1 | 1, 2, 3, 10 | ditar de verdade e print do item; consulta SQLite antes e depois | 4 | passou (local): 22 testes; na janela Tauri real (Xvfb) editar gravou editado=1 com original preservado, excluir em 2 passos apagou a linha; retenção tirou o item de 40 dias (print). Corrigido: botão Confirmar exclusão ilegível no hover. Falta: ditado real gravando e Windows; Colar fica para a tarefa 3 |
| 3 | PR 1 · Colar último: menu da bandeja (Windows e Linux) + atalho · Sonnet | 1, 2 | 4 | atalho num editor de texto, print | 4 | passou (local): Alt+Win+V real colou o último ditado com acentos num editor (Xvfb, print). Falta: item do menu e Windows (conflito do Alt+Win+V) |
| 4 | PR 1 · Aparência: luz, tema, notificações · Sonnet | 1 | 9 | print da luz em cada posição e do tema | 4 | passou (local): cliques reais em Escuro/Verde/Topo/Grossa gravaram no ajustes.json; luz verde no topo (Xvfb, sem compositor). Falta: Windows |
| 5 | PR 2 · Tela Início (layout B) + estatísticas · Sonnet | 2 | 11 | print com dados reais do banco | 4 | passou (local): janela Tauri real (Xvfb) com histórico semeado mostra palavras hoje/semana, tempo poupado, gráfico de 7 dias e último ditado (print pr2-inicio). Windows 11: passou (prova delegada, 27/11) |
| 6 | PR 2 · Sugestões de termos (correção + frequência), aceitar/ignorar · Sonnet | 2, 5 | 12, 13 | teste com histórico de exemplo + print | 4 | passou (local): nome repetido 5+ vezes vira sugestão; Aceitar gravou no vocabulario.txt e `aceita`; Ignorar gravou `ignorada`; troca na correção do Histórico oferece "Adicionar ao vocabulário" (prints). Corrigido: nome composto virava dois termos; caixa da troca saía do painel. Windows 11: passou. Falta: motivo `correcao` na tabela (a troca só aparece ao salvar a correção) |
| 7 | PR 2 · Substituições (comandos + atalhos próprios) · Sonnet | 2 | 14 | teste unitário da função + ditado real | 3 | passou (local): 63 testes; fluxo `transcrever_e_colar` com transcrição simulada colou regra própria, vírgula desligada e "ponto final, nova linha" com maiúscula; histórico guarda colado e original; aba Substituições gravou comando e regra pelo clique. Windows 11: passou. Falta: ditado real com microfone |
| 8 | PR 3 · IA no texto: modos, modo por aplicativo, traduzir, fallback literal · Sonnet | 1, 2 | 15, 16, 17, 18 | ditado real em cada modo + teste de timeout | 3 | passou (local): 13 testes da escrita (modo por app pelo trecho mais longo, literal sem rede, falha e >4 s colam o literal, log sem conteúdo); fluxo `transcrever_e_colar` simulado (Outlook → formal, traduzir → inglês, IA fora do ar → literal e bipe grave); aba Escrita com IA e atalho de traduzir pelo clique (Xvfb, prints). Windows 11 (27/11): janela e Shift + Win passaram (layout do teclado intacto); IA real falhou com o llama desligado (404) → modelo trocado para `openai/gpt-oss-120b`, que na cópia do Windows reescreveu formal e traduziu (0,4–0,8 s) e não obedeceu injeção. Falha da IA agora dá bipe grave. Falta: rodada final no Windows com o commit do modelo novo |
| 9 | Cada PR · README, `verificar-ditado`/`verificar-janela`, versão na janela | cada PR | — | `verificacao-manter` | — | pendente |

## Ações críticas (só nível 3)
| # | Ação | Alvo real (ambiente, tabela, registros) | Simulação | Backup conferido | Plano de volta | Executada em |
|---|---|---|---|---|---|---|

## Revisão de código do PR 1 (subagente Sonnet, 26/11/2026)
- Corrigidos: microfone desligado com o fluxo aberto agora reabre no padrão (antes falhava calado); sem microfone nenhum → luz e bipe de erro; atalho contido no outro (ctrl+win × ctrl+win+v) recusado no Rust e no Python; Ctrl+C/V/X/Z/A/S, Alt+F4 e atalho só com Shift recusados; F01/f+1 recusados no Rust; atalho recusado não perde o resto do lote (volume, microfone) e a UI volta ao atalho gravado (print `depois-atalho-recusado.png`); correção em edição no Histórico sobrevive a busca e troca de aba (print `depois-rascunho-preservado.png`); cola antes de gravar no histórico; rename do ajustes.json com 3 tentativas (Windows).
- Não corrigidos (registrados): busca não ignora acento ("agua" não acha "água"); lista de microfones só atualiza ao reiniciar o app; volume 0 também cala o bipe de erro (decisão: 0 = mudo).
- Sem achado: XSS, SQL injection, chaves desconhecidas do ajustes.json, retenção, fluxo antigo (`configuracoes.py`).

## Prova no Windows 11 (notebook, delegada, sobre 41a4c94)
- Passou: `cargo build --release` (5,7 MiB); bipe 60/30/0 (pico 0,6/0,3/sem tocar); `app_ativo()`; `keyboard` + Casador com eventos sintéticos (Ctrl+Win inicia/para, Alt+Win+V com repetição → um só colar); luz verde grossa no topo; notificações liga/desliga; Alt+Win+V livre no `RegisterHotKey`.
- Achados corrigidos depois (4dca455): testes isolam `APPDATA` (antes regravavam o ajustes.json real), testes Linux com skip no Windows, Shift+número pelo scan_code, lista de microfones só com entradas que abrem a 16 kHz.
- Não provado: som audível, tecla real e colagem (PC em uso), menu "Colar último ditado", janela Tauri aberta. Rótulo "Ctrl+Win" fixo na dica e no menu (B5); `release.yml` não roda testes.

## Qualidade do PR 1 (26/11/2026)
- Simplificação: 2 passadas (/simplify + karpathy). 1ª: `comum.log` único, `_gravar_json`, laço dos dois atalhos, `historico.gravar` direto. 2ª: lista de microfones voltou a aparecer (`sd` não importado em `_abre_a_16k`), sai o `lru_cache` de `validar_atalho`, sai try redundante de `app_ativo`.
- Framework: sem skill de framework para Python/Tauri.
- /code-review high --fix: confirmação de exclusão não passa para outro item ao buscar (print `depois-exclusao-desarma-na-busca.png`); luz desligada no meio do ditado apaga; Configurações abre em thread; volume NaN/Infinity → padrão. Não corrigidos: busca de microfone por nome sem o filtro de 16 kHz; atualização manual muda com notificações desligadas.
- Verificação: skills `verificar-janela` e `verificar-ditado` criadas; 39 testes Python + 2 Rust OK; Alt+Win+V colou o último ditado num editor GTK (Xvfb).

## Pendências e fora de escopo
- Microfone por nome pode cair numa API que não abre a 16 kHz (cai no padrão).
- "Procurar atualização" manual sem retorno quando as notificações estão desligadas.
- Busca do Histórico sem ignorar acento.
- Provar por clique na janela Tauri as abas novas (tela estava bloqueada em 26/11 22:18).
- Provar no Windows: bipe gerado, atalho via `keyboard` + Casador.
- Microfone plugado com o app aberto só aparece após reiniciar (cache do PortAudio).
- Prova de janela: Xvfb em :99 + xdotool (não usar a tela real). Disco raiz em 95% (scratchpads de outras sessões).
- App aparece como classe/exe (`code`, `outlook`), sem nome amigável.
- Rótulo "Segure Ctrl + Super" no menu da bandeja Linux ainda fixo.
- PR 2: Início sem botão Colar (só Copiar + dica do atalho); a janela não tem comando para colar no programa em uso.
- PR 2: sugestões por correção não vão para a tabela (só a caixa no Histórico ao salvar).
- PR 2: salvar Substituições recusa tudo se o arquivo editado à mão tiver regra duplicada; comentários do arquivo se perdem ao salvar pela janela.
- PR 2: limpeza de espaços dos comandos vale para o texto todo quando algum comando dispara.
- PR 2: aba padrão `inicio` exige Python e exe de configurações da mesma versão (atualização parcial abriria a janela sem painel).
- PR 8: no Windows, dois processos do mesmo app com volumes diferentes voltam ambos ao volume de um deles.

- PR 3: modo Formal com `gpt-oss-120b` pode cortar a saudação ou acrescentar palavras (1 amostra no Windows); ajustar a instrução se repetir.
- Whisper com áudio quase mudo colou "Obrigado." (visto no Windows; não verificado se é anterior).
- PR 3: salvar um atalho pode acusar conflito com o de traduzir mesmo com ele desligado (o Rust compara com o padrão).
- PR 3: Assumi: modo padrão Literal (sem custo nem mudança), traduzir em Shift + Win (Alt + Shift troca o idioma do teclado no Windows), modelo `openai/gpt-oss-120b` (o `llama-3.3-70b-versatile` foi desligado pela Groq em 16/08/2026; a prova no Windows pegou), temperatura 0,2, modo por app pelo trecho do nome do programa.
- Abas Ditado e som e Escrita com IA não rolavam (cards cortados): corrigido no PR 3.

## Retro
- Modelo da Groq desligado (llama-3.3-70b) só apareceu na prova Windows: no Linux não há chave, então a IA real nunca rodou antes do notebook.
- Abas novas (Ditado e som, Escrita) nasceram sem rolagem: a lista de painéis roláveis no CSS é manual e ninguém lembrou de incluir.
- Atalho padrão alt+shift+win trocaria o idioma do teclado no Windows: atalho escolhido sem checar combinações reservadas do sistema.
- Prova de PR 1 no Windows ficou parcial (tela bloqueada), e o #6 seguiu em rascunho até o fim.

### Prova Windows final do PR 3 (27/11/2026, af9ac55)
- gpt-oss-120b ativo; 4 modos reais 0,41–0,94 s; injeção não obedecida; Shift+Win colou inglês 5/6; notepad formal ok; chave inválida → literal + log + bipe; 76 testes OK.
- Pendências novas: 1 execução do Shift+Win não colou (histórico certo, causa desconhecida); `pycaw`/`comtypes` não importa no Python 3.14 do notebook, então "abaixar os outros sons" provavelmente não funciona lá.
