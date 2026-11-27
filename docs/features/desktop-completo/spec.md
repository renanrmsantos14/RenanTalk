# Spec: RenanTalk desktop mais completo

## Objetivo
O RenanTalk passa a guardar os ditados, aprender os termos que você usa, ter controle de som, microfone, idioma e atalho, e reescrever o texto com IA quando você quiser, sem perder o atalho Ctrl + Win nem a leveza (~30 MB parado).

## Fora de escopo
Mãos livres, pausar música, ajustes de colagem, exportar/importar, sincronizar máquinas, pausar histórico, Whisper offline, guardar áudio.

## Dados (arquivos locais, pasta `PASTA` de `app/comum.py`)
- `ajustes.json`: volume (0–100), microfone (nome ou padrão), idioma (`pt` | `auto` | `en`), atalhos (ditar, colar último, traduzir), luz (cor, espessura, posição `baixo`/`topo`, ligada), tema (`sistema`/`claro`/`escuro`), notificações (ligadas), modo de escrita padrão, modos por aplicativo, retenção (30 dias). O Python relê o arquivo a cada ditado; a janela grava nele.
- `historico.db` (SQLite, modo WAL; Python grava, a janela Rust lê e edita por `rusqlite` com `bundled`): `ditado(id, criado_em, texto, original, app, modo, idioma, duracao_s, palavras, editado)`; `sugestao(termo, motivo, contagem, estado)` com estado `nova`/`aceita`/`ignorada`. Itens com mais de 30 dias são apagados na abertura do app e uma vez por dia.
- `substituicoes.txt`: `quando eu disser => escreva`, uma por linha; comandos prontos (nova linha, ponto final, vírgula, ponto de interrogação, abre/fecha parênteses) vêm ativos e podem ser desligados.
- Promessa do README muda: os textos ficam no computador por 30 dias; áudio continua sem ser gravado; log continua sem conteúdo.

## Cenários de aceite (ação → resultado)
PR 1, base
1. Ditar → o item aparece no Histórico com hora, aplicativo, modo, idioma e duração.
2. Editar o texto de um item e salvar → fica marcado "editado" e o original continua guardado.
3. Copiar e Colar num item → texto na área de transferência ou colado na janela anterior. Excluir e Apagar tudo → somem do banco.
4. Menu da bandeja "Colar último ditado" e atalho (padrão assumido Alt + Win + V; conferir conflito no Windows 11) → cola o último texto.
5. Volume a 30% → os três bipes saem mais baixos (Windows e Linux), prévia ao soltar o controle; 0% → mudo.
6. Escolher microfone → o próximo ditado usa esse dispositivo; dispositivo sumiu → volta ao padrão e registra no log.
7. Idioma inglês ou automático → enviado ao Whisper (`language` omitido no automático).
8. Trocar o atalho de ditar → passa a valer sem reiniciar; atalho inválido ou repetido → recusado com mensagem.
9. Luz: cor, espessura, topo ou embaixo, ou desligada → vale no próximo ditado. Tema claro/escuro/sistema → a janela troca na hora. Notificações desligadas → a bandeja não avisa (erro continua no bipe grave e no log).
10. Item de mais de 30 dias → apagado.

PR 2, inteligência local
11. A janela abre na tela Início: palavras hoje e na semana, tempo poupado (estimativa: palavras ÷ 40 por minuto digitando, menos o tempo falado), último ditado (copiar/colar), gráfico de 7 dias, sugestões.
12. Corrigir "renan talk" → "RenanTalk" no Histórico → a comparação mostra a troca e oferece adicionar o termo ao vocabulário.
13. Nome ou sigla com maiúscula repetido 5 vezes ou mais em 30 dias, fora do vocabulário e de uma lista de palavras comuns → vira sugestão. Ignorada → não volta.
14. Dizer "nova linha" ou um atalho cadastrado → o texto colado já vem substituído; o histórico guarda o texto final e o original.

PR 3, IA no texto (modelo de texto da Groq, mesma chave)
15. Modo revisado, formal/e-mail ou WhatsApp → o texto é reescrito antes de colar; literal não chama a IA.
16. Modo por aplicativo (Outlook → formal, WhatsApp → WhatsApp, VS Code → literal; editável) → escolhido pelo programa ativo.
17. Atalho de traduzir → cola em inglês.
18. IA falhou ou demorou mais de 4 s → cola o texto literal e registra no log (nunca perde o ditado).

## O que depende do que muda
- `ditado.pyw` (transcrever/colar), `plataforma_windows.py` e `plataforma_linux.py` (bipe, atalho, luz, bandeja), `bandeja.py` (menu Windows), `config/src/main.rs` + `app/ui/index.html` (janela), `app/configuracoes.py` (janela antiga Python+Edge: recebe só o que for necessário para não quebrar; recursos novos só na janela Tauri), `release.yml` (binário Tauri maior com SQLite), README.
- Vocabulário atual (`vocabulario.txt`) continua sendo a fonte; sugestões aceitas são acrescentadas nele.

## Irreversível
Nada. Apagar histórico é ação do usuário, com confirmação.
