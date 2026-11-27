---
name: verificar-ditado
description: "Verifica o app da bandeja do RenanTalk desktop (Python): atalhos, colar último ditado, gravação no histórico, ajustes.json, luz. Use após mudar app/*.py, app/ditado.pyw ou tests/**, ou quando pedirem para testar o ditado."
---
# Verificar ditado (app da bandeja)

## Cobre
- Código: `app/ditado.pyw`, `app/ajustes.py`, `app/historico.py`, `app/comum.py`, `app/luz.py`, `app/bandeja.py` (Windows), `app/plataforma_linux.py`, `app/plataforma_windows.py`, `app/substituicoes.py`, `app/sugestoes.py`, `app/escrita.py`.

## Preparar ambiente
- Testes: `python3 -m unittest discover -s tests` → `OK` (39+ testes; isolam `XDG_CONFIG_HOME`/`APPDATA`).
- **Nunca na tela real**: Xvfb `:99` (veja `verificar-janela`). `export DISPLAY=:99 XDG_CONFIG_HOME=<scratchpad>/vm` com o histórico semeado como em `verificar-janela`.
- Subir: `(timeout 40 python3 app/ditado.pyw >$S/dit.log 2>&1 &); sleep 5` → `$XDG_CONFIG_HOME/RenanTalk/ditado.log` tem "iniciado v…". Os erros de ALSA no stdout são normais (Xvfb sem áudio).
- Editor de teste (não há tkinter nem gedit): janela GTK
  ```python
  import gi; gi.require_version("Gtk","3.0"); from gi.repository import Gtk
  w=Gtk.Window(title="Editor"); w.set_default_size(600,160); w.move(50,300)
  t=Gtk.TextView(); w.add(t); w.connect("destroy",Gtk.main_quit); w.show_all(); Gtk.main()
  ```

## Roteiro
1. Testes unitários → `OK`.
2. Colar último: abrir o editor, `xdotool mousemove 300 380 click 1; xdotool key alt+super+v` → o editor recebe "Ligar para o motorista do aeroporto" com acentos intactos e o log mostra "colar último: colado".
3. Área de transferência depois de colar (`plat.colar`, com `Gtk.main` numa thread): vazia antes → vazia depois (`wait_for_text()` = None, o ditado não fica); com "meu texto" antes → "meu texto" depois. No Windows: texto e imagem (Print Screen) copiados antes voltam iguais, e o ditado não aparece no Win+V.
3. Trocar `atalho_ditar` no `ajustes.json` com o app aberto → o listener passa a usar o novo atalho sem reiniciar (cache por mtime).
4. `ajustes.json` corrompido (`{`, `"volume": NaN`, atalho `["alt","win"]`) → o app segue com os padrões e registra no log.
5. Abaixar os outros sons (Linux): `abaixar_outros: true, volume_outros: 30` no `ajustes.json`; um fluxo de teste em laço (`while true; do paplay --client-name=TesteRenanTalk silencio.wav; done &`, wav de silêncio gerado com `sox -n -r 16000 silencio.wav trim 0 2` ou numpy+wave); `xdotool keydown ctrl super`, 2 s, `pactl -f json list sink-inputs` → os outros fluxos a 30% (65536 → 19661) e o `PipeWire ALSA [python3.x]` do próprio app intacto; `xdotool keyup super ctrl`, 2 s → todos de volta ao valor de antes e estáveis 3 s depois. Com o ajuste desligado nada muda. App que fecha no meio: com um wav de 1 s no laço (o fluxo troca durante o ditado), segurar 3,5 s → ao soltar, o fluxo novo do `TesteRenanTalk` vem a 65536, não a 30% (o PipeWire guarda o volume por app). Anote o volume do Spotify antes e confira depois (ele reage à mudança externa).

## Substituições e sugestões (sem microfone)
- Fluxo inteiro com a transcrição simulada: carregar `app/ditado.pyw` com `importlib.machinery.SourceFileLoader`, trocar `d.transcrever` por uma função que devolve `"Mande para meu email vírgula ok ponto final nova linha obrigado"`, `d.plat.colar` por uma lista e `d.plat.app_ativo` por `lambda: "teste"`; chamar `d.transcrever_e_colar(b"x", 2.0)`.
- Com `substituicoes.txt` = `meu email => contato@exemplo.com` e `comandos.virgula: false` no `ajustes.json` → colado `"Mande para contato@exemplo.com vírgula ok.\nObrigado "`; `historico.listar(limite=1)[0]` tem `texto` = colado e `original` = o texto cru.
- Sugestões: gravar o mesmo nome 5 vezes (`historico.gravar("Buscar no Santos Dumont …")`) → tabela `sugestao` com `('Santos Dumont', 'frequencia', 5, 'nova')`; 4 vezes não sugere; nome composto vira um termo só; o que já está no vocabulário não entra.
- Defeito numa regra não perde o ditado: o texto cru é colado e o log diz "substituições falharam".

## IA no texto (sem chave nem rede)
- `tests/test_escrita.py` cobre modo por app, fallback e o atalho de traduzir. Fluxo: no mesmo esquema acima, `mock.patch("urllib.request.urlopen", return_value=<BytesIO com {"choices":[{"message":{"content":"..."}}]}>)` e `d.plat.app_ativo` → `"OUTLOOK.EXE"` → colado o texto da IA e `historico.gravar(..., modo="formal")`; `d.transcrever_e_colar(b"", 1.0, True)` → modo `ingles` em qualquer app; `urlopen` com `side_effect=OSError` → colado o texto cru, modo `literal` e o log diz "IA (…) falhou" sem o conteúdo.
- Atalho de traduzir (padrão `shift+win`): `xdotool keydown shift super`, falar, `keyup` → mesmo ciclo do ditar com `em_ingles=True`.

## Não dá para provar aqui
- IA real (modelo `openai/gpt-oss-120b` em `/openai/v1/models` (o llama-3.3-70b foi desligado em 16/08/2026; teste só com mock não pega modelo removido), cada modo, chave inválida → literal): não há chave no Linux; provar no Windows.
- Ditado real (microfone + Groq): Xvfb não tem áudio. Provar numa máquina com microfone e chave.
- Windows (`keyboard` + Casador, `winsound`, bandeja Win32, abaixar outros sons via pycaw): delegar ao notebook `Betinhos-Renan`.

## Evidências
- `.verificacao/<data>-<branch>/` (prints e o trecho do `ditado.log`).
