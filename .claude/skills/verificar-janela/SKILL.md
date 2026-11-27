---
name: verificar-janela
description: "Verifica a janela de configurações do RenanTalk desktop (Tauri + app/ui/index.html): histórico, ditado e som, aparência, atalhos. Use após mudar app/ui/**, config/** ou app/configuracoes.py, ou quando pedirem para testar a janela."
---
# Verificar janela de configurações

## Cobre
- Código: `config/src/main.rs` (comandos Rust, SQLite, ajustes.json), `app/ui/index.html` (UI embutida no binário).
- Abas: Início, Histórico, Vocabulário, Substituições, Ditado e som, Aparência, Chave Groq, Sobre.

## Preparar ambiente
- **Nunca na tela real.** Use o Xvfb em `:99` (`pgrep -a Xvfb`; se não houver: `Xvfb :99 -screen 0 1024x768x24 &`).
- A UI é embutida no build: depois de mudar `app/ui/index.html` ou o Rust, `cd config && cargo build --release -q`.
- Testes do Rust: `cd config && cargo test -q` (2 testes de atalho).
- Dados isolados: `S=<scratchpad>/vm; export DISPLAY=:99 XDG_CONFIG_HOME=$S` (o app usa `$XDG_CONFIG_HOME/RenanTalk/`). Semear o histórico:
  ```bash
  python3 -c "import sys; sys.path.insert(0,'app'); import historico
  historico.gravar('Reunião com a diretoria às dez', app='outlook')
  historico.gravar('Mandar o orçamento da van executiva', app='code')
  historico.gravar('Ligar para o motorista do aeroporto', app='whatsapp')"
  ```
- Abrir: `pkill -x renantalk-confi; (timeout 60 config/target/release/renantalk-config $$ hist python3 app &); sleep 6` (3º argumento = aba inicial: `inicio` (padrão), `hist`, `sub`, `som`, `apar`…). Para gerar sugestões, semeie um nome 5+ vezes (ex.: `for i in range(6): historico.gravar(f'Buscar o cliente no Santos Dumont às {i+8}h', app='whatsapp', duracao_s=4)`). Tire o mouse de cima antes do print (`xdotool mousemove 1000 10`): o tooltip das barras sai como retângulo preto no Xvfb.
- Janela: `w=$(xdotool search --name '^RenanTalk$' | tail -1); eval $(xdotool getwindowgeometry --shell $w)`; clique relativo: `c(){ xdotool mousemove $((X+$1)) $((Y+$2)) click 1; sleep .8; }`; print: `import -window $w <arquivo>.png`. Janela 800x580.

## Roteiro
1. Abrir em `hist` → lista "HOJE" com os 3 ditados, o mais novo selecionado, detalhe com "App: whatsapp", "Modo: Literal", "6 palavras".
2. `c 684 518` (Excluir) → botão vira "Confirmar exclusão" vermelho.
3. Com a confirmação armada, `c 378 133` (busca) e `xdotool type "van"` → só "Mandar o orçamento…" na lista e o botão volta a "Excluir" (a confirmação não passa para outro item).
4. Editar o texto no detalhe e clicar "Salvar correção" → `sqlite3 $S/RenanTalk/historico.db "select editado, original from ditados"` mostra `editado=1` e o original preservado.
5. Excluir em 2 cliques → a linha some do banco.
6. Aba Ditado e som: gravar atalho ctrl+alt → `ajustes.json` muda; atalho proibido (Ctrl+C) ou contido no outro → aviso e o cartão volta ao atalho gravado, o volume do mesmo lote é gravado. Marcar "Abaixar os outros sons enquanto fala" libera o controle "Ficam em" (desabilitado com a caixa desmarcada); mover para 20% → `abaixar_outros: true, volume_outros: 20` no `ajustes.json`.
7. Aba Aparência: Escuro / Verde / Topo / Grossa → gravados em `ajustes.json` (`luz`, `tema`).

8. Aba Início (abre por padrão): cartões Palavras hoje / na semana / Tempo poupado, gráfico de 7 dias com "hoje" em destaque, Último ditado com Copiar. Rolar (`xdotool click 5` sobre o painel) → "Termos que você repete" com "Santos Dumont · Apareceu 6 vezes". Aceitar → o termo vai ao fim do `vocabulario.txt` e `sugestao.estado = 'aceita'`; Ignorar → `ignorada` e some da lista. Sem ditados → mensagem de vazio.
9. Aba Substituições: desmarcar "Vírgula" → `ajustes.json` `comandos.virgula: false`. Em "Suas regras", digitar "meu email" e "contato@exemplo.com" e Adicionar → linha `meu email => contato@exemplo.com` em `substituicoes.txt`; regra com `\n` aparece como `\n` literal na lista. O arquivo é lido ao abrir a janela.
10. Histórico: editar um ditado trocando uma palavra (ex.: "van" → "Sprinter") e Salvar correção → caixa "Troca: “van” → “Sprinter”" visível sem rolar, botões dentro da caixa; "Adicionar ao vocabulário" grava o termo. Apagar tudo também zera a tabela `sugestao`.
11. Aba Escrita com IA (`esc`): trocar o Modo padrão para Revisado → `ajustes.json` `modo: revisado` e selo "Salvo"; digitar "Slack" em Aplicativo e Enter → `modos_apps.slack` (minúsculas); ✕ numa linha a remove do arquivo. Padrões: code, olk, outlook, whatsapp.
12. Aba Ditado e som rola até o card "Atalho de traduzir" (padrão Shift + Win); Ctrl + Alt + Shift + Win → aviso "se confunde com o de ditar"; Alt + Shift → gravado.

## Casos de borda
- Busca não ignora acento ("agua" não acha "água") — conhecido, pendente.
- Rascunho da correção sobrevive à busca e à troca de aba.

## Evidências
- Prints em `.verificacao/<data>-<branch>/`; prints de PR em `docs/features/<slug>/prints/`.
