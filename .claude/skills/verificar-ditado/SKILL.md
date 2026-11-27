---
name: verificar-ditado
description: "Verifica o app da bandeja do RenanTalk desktop (Python): atalhos, colar último ditado, gravação no histórico, ajustes.json, luz. Use após mudar app/*.py, app/ditado.pyw ou tests/**, ou quando pedirem para testar o ditado."
---
# Verificar ditado (app da bandeja)

## Cobre
- Código: `app/ditado.pyw`, `app/ajustes.py`, `app/historico.py`, `app/comum.py`, `app/luz.py`, `app/bandeja.py` (Windows), `app/plataforma_linux.py`, `app/plataforma_windows.py`.

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
3. Trocar `atalho_ditar` no `ajustes.json` com o app aberto → o listener passa a usar o novo atalho sem reiniciar (cache por mtime).
4. `ajustes.json` corrompido (`{`, `"volume": NaN`, atalho `["alt","win"]`) → o app segue com os padrões e registra no log.

## Não dá para provar aqui
- Ditado real (microfone + Groq): Xvfb não tem áudio. Provar numa máquina com microfone e chave.
- Windows (`keyboard` + Casador, `winsound`, bandeja Win32): delegar ao notebook `Betinhos-Renan`.

## Evidências
- `.verificacao/<data>-<branch>/` (prints e o trecho do `ditado.log`).
