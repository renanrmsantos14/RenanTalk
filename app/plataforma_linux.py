"""Peças do RenanTalk específicas do Linux (X11; testado em Linux Mint / Cinnamon).

- Atalho Ctrl+Super: extensão RECORD do X11 (não precisa de root nem do grupo input).
- Colar: área de transferência do GTK + Ctrl+V (Ctrl+Shift+V em terminais) via XTest.
- Luz: janela GTK transparente, acima de tudo e sem receber clique, logo acima do painel.
- Bandeja: XApp.StatusIcon (Cinnamon); na falta, AyatanaAppIndicator3 ou Gtk.StatusIcon.
"""
import fcntl
import json
import os
import subprocess
import sys
import threading
import time

import cairo
import gi
import numpy as np

gi.require_version("Gtk", "3.0")
gi.require_version("Gdk", "3.0")
from gi.repository import Gdk, GLib, Gtk  # noqa: E402
from Xlib import X, XK, display  # noqa: E402
from Xlib.ext import record, xtest  # noqa: E402
from Xlib.protocol import rq  # noqa: E402

import ajustes  # noqa: E402
from comum import (ARQ_LOG, ICONE_PNG, NOME, PASTA, abrir, abrir_configuracoes,  # noqa: E402
                   definir_inicio_automatico, inicio_automatico, log)

TERMINAIS = ("terminal", "xterm", "urxvt", "kitty", "alacritty", "konsole", "tilix", "terminator", "wezterm", "foot")
_trava_arquivo = None


# ---------- utilidades ----------
def bipe(freq: int, ms: int) -> None:
    ajustes.bipe(freq, ms)


def _sink_inputs() -> list[dict]:
    r = subprocess.run(["pactl", "-f", "json", "list", "sink-inputs"], capture_output=True, timeout=2,
                       env={**os.environ, "LC_ALL": "C"}, check=True)
    return json.loads(r.stdout)


def _definir_volume(indice: int, canais: list[int]) -> None:
    subprocess.run(["pactl", "set-sink-input-volume", str(indice), *map(str, canais)], timeout=2, check=True)


def abaixar_outros(fator: float) -> list:
    """Põe cada app que toca som em `fator` do volume atual. Devolve o que mudou, para restaurar_outros.

    O bipe de início ainda pode estar tocando: pula o fluxo do nosso processo quando o servidor informa o
    `application.process.id`. O bipe de fim sai num fluxo novo, que não está nesta lista."""
    feitos = []
    try:
        entradas = _sink_inputs()
    except (OSError, ValueError, subprocess.SubprocessError) as e:
        log(f"abaixar volume: {e!r}")
        return feitos
    for e in entradas:  # um fluxo que sumiu ou sem volume não impede abaixar os outros
        try:
            if str(e.get("properties", {}).get("application.process.id")) == str(os.getpid()):
                continue  # os bipes do RenanTalk ficam no volume dos ajustes
            antes = [c["value"] for c in e["volume"].values()]
            if not antes:
                continue
            _definir_volume(e["index"], [round(v * fator) for v in antes])
            feitos.append((e["index"], antes))
        except (OSError, KeyError, TypeError, AttributeError, subprocess.SubprocessError) as err:
            log(f"abaixar volume: {err!r}")
    return feitos


def restaurar_outros(feitos: list) -> None:
    """Volta sempre o volume de antes. Não dá para respeitar o que mudou no meio: o Spotify reage à mudança
    de fora e mexe no próprio volume, e ficaria no valor errado."""
    for indice, antes in feitos:
        try:
            _definir_volume(indice, antes)
        except (OSError, subprocess.SubprocessError) as e:  # o app fechou durante o ditado
            log(f"restaurar volume: {e!r}")


def instancia_unica() -> bool:
    global _trava_arquivo
    _trava_arquivo = open(os.path.join(PASTA, ".instancia"), "w")
    try:
        fcntl.flock(_trava_arquivo, fcntl.LOCK_EX | fcntl.LOCK_NB)
        return True
    except OSError:
        return False


def liberar_instancia() -> None:
    if _trava_arquivo:
        _trava_arquivo.close()


# ---------- colar ----------
_xd = display.Display()


def _classe_janela_ativa() -> str:
    try:
        raiz = _xd.screen().root
        ativa = raiz.get_full_property(_xd.intern_atom("_NET_ACTIVE_WINDOW"), X.AnyPropertyType)
        jan = _xd.create_resource_object("window", ativa.value[0])
        classe = jan.get_wm_class() or ()
        return " ".join(classe).lower()
    except Exception:  # noqa: BLE001
        return ""


def app_ativo() -> str:
    """Nome do aplicativo em primeiro plano (classe WM, ex.: 'code'); vazio se não der para saber."""
    return (_classe_janela_ativa().split() or [""])[-1]


def _teclar(*keysyms: int) -> None:
    codigos = [_xd.keysym_to_keycode(k) for k in keysyms]
    for c in codigos:
        xtest.fake_input(_xd, X.KeyPress, c)
    for c in reversed(codigos):
        xtest.fake_input(_xd, X.KeyRelease, c)
    _xd.sync()


def colar(texto: str) -> None:
    """Chamado fora da thread do GTK: agenda tudo no GTK e espera terminar."""
    pronto = threading.Event()
    estado = {}

    def copiar():
        clip = Gtk.Clipboard.get(Gdk.SELECTION_CLIPBOARD)
        estado["anterior"] = clip.wait_for_text()
        clip.set_text(texto, -1)
        clip.store()
        pronto.set()
        return False

    GLib.idle_add(copiar)
    pronto.wait(3)
    time.sleep(0.05)
    terminal = any(t in _classe_janela_ativa() for t in TERMINAIS)
    if terminal:
        _teclar(XK.XK_Control_L, XK.XK_Shift_L, XK.XK_v)
    else:
        _teclar(XK.XK_Control_L, XK.XK_v)
    time.sleep(0.3)
    if estado.get("anterior") is not None:  # devolve o que estava na área de transferência
        GLib.idle_add(lambda: (Gtk.Clipboard.get(Gdk.SELECTION_CLIPBOARD).set_text(estado["anterior"], -1), False)[1])


# ---------- atalho ----------
def _codigos(local, alvo) -> dict[int, str]:
    """keycode -> nome ('ctrl', 'alt', 'shift', 'win' e a tecla do atalho, se houver)."""
    mapa: dict[int, str] = {}
    grupos = {"ctrl": (XK.XK_Control_L, XK.XK_Control_R), "alt": (XK.XK_Alt_L, XK.XK_Alt_R),
              "shift": (XK.XK_Shift_L, XK.XK_Shift_R), "win": (XK.XK_Super_L, XK.XK_Super_R)}
    for nome, keysyms in grupos.items():
        for k in keysyms:
            mapa[local.keysym_to_keycode(k)] = nome
    for nome in alvo:
        if nome not in grupos:
            ks = XK.string_to_keysym(nome) or XK.string_to_keysym(nome.capitalize())
            if ks:
                mapa[local.keysym_to_keycode(ks)] = nome
    return mapa


def rodar(ao_iniciar, ao_parar, ao_outra, esta_gravando, ao_colar=None) -> None:
    """Escuta os atalhos de ditar, traduzir e colar o último (RECORD do X11) numa thread e roda o laço do GTK na principal."""
    def escutar() -> None:
        local, rec = display.Display(), display.Display()
        casador = ajustes.Casador()
        estado = {"alvo": None, "codigos": {}}
        ctx = rec.record_create_context(0, [record.AllClients], [{
            "core_requests": (0, 0), "core_replies": (0, 0, 0, 0), "ext_requests": (0, 0, 0, 0),
            "ext_replies": (0, 0, 0, 0), "delivered_events": (0, 0),
            "device_events": (X.KeyPress, X.KeyRelease), "errors": (0, 0),
            "client_started": False, "client_died": False}])

        def ao_registro(reply) -> None:
            if reply.category != record.FromServer or reply.client_swapped or not reply.data:
                return
            a = ajustes.ler()  # relido: vale sem reiniciar
            alvos = tuple(ajustes.validar_atalho(a[c]) for c in ajustes.ATALHOS)  # ditar, colar, traduzir
            if alvos != estado["alvo"]:
                estado["alvo"], estado["codigos"] = alvos, _codigos(local, sum((t or () for t in alvos), ()))
            dados = reply.data
            while dados:
                ev, dados = rq.EventField(None).parse_binary_value(dados, rec.display, None, None)
                if ev.type not in (X.KeyPress, X.KeyRelease):
                    continue
                nome = estado["codigos"].get(ev.detail, "outra")
                ditar, colar, traduzir = alvos
                r = casador.evento(nome, ev.type == X.KeyPress, ditar, esta_gravando(), colar, traduzir)
                if r == "colar" and ao_colar:
                    ajustes.colar_ao_soltar(casador, ao_colar)
                elif r in ("iniciar", "traduzir"):
                    ao_iniciar(r == "traduzir")
                elif r == "parar":
                    ao_parar()
                elif r == "outra":
                    ao_outra(f"keycode={ev.detail}")

        rec.record_enable_context(ctx, ao_registro)  # bloqueia

    threading.Thread(target=escutar, daemon=True).start()
    Gtk.main()


# ---------- luz ----------
ALTURA = 36
ERRO = ((254 / 255, 202 / 255, 202 / 255), (248 / 255, 113 / 255, 113 / 255), (220 / 255, 38 / 255, 38 / 255))


class Luz:
    """Mesma linha de luz do Windows: nasce no centro, forte no meio, apaga nas pontas.

    Cor, espessura, posição (embaixo/topo) e liga/desliga vêm de ajustes.json, lidos a cada ditado."""

    def __init__(self) -> None:
        self.estado, self.t0, self.alvo, self.nivel, self.timer = "idle", time.time(), 0.0, 0.0, None
        self.ligada, self.k, self.topo, self.altura = True, 1.0, False, ALTURA
        self.cores = {n: tuple(c / 255 for c in rgb) for n, rgb in ajustes.CORES_LUZ["azul"].items()}
        self.jan = Gtk.Window(type=Gtk.WindowType.POPUP)  # override-redirect: sem foco e sem barra de tarefas
        self.jan.set_app_paintable(True)
        visual = self.jan.get_screen().get_rgba_visual()
        if visual:
            self.jan.set_visual(visual)
        self.jan.set_accept_focus(False)
        self.jan.connect("draw", self._desenhar)
        self.jan.connect("realize", lambda w: w.get_window().input_shape_combine_region(cairo.Region(), 0, 0))

    def definir(self, estado: str) -> None:
        GLib.idle_add(self._definir, estado)

    def audio(self, v: float) -> None:
        self.alvo = v

    def _configurar(self) -> None:
        """Lê os ajustes da luz; vale a partir deste ditado."""
        cfg = ajustes.ler()["luz"]
        self.ligada, self.topo = cfg["ligada"], cfg["posicao"] == "topo"
        self.k = ajustes.ESPESSURAS[cfg["espessura"]]
        self.altura = round(ALTURA * self.k)
        self.cores = {n: tuple(c / 255 for c in rgb) for n, rgb in ajustes.CORES_LUZ[cfg["cor"]].items()}

    def _definir(self, estado: str) -> bool:
        if estado == "rec":
            self._configurar()
        if not self.ligada:  # luz desligada: não mostra a janela (o bipe e o resto do ditado seguem normais)
            if self.estado != "idle":  # e apaga a luz do ditado anterior que ainda estivesse na tela
                self.estado = "idle"
            return False
        if estado == "rec":
            self.nivel = 0.0
            self._posicionar()
            self.jan.show()
        self.estado, self.t0 = estado, time.time()
        if not self.timer:
            self.timer = GLib.timeout_add(25, self._tique)
        return False

    def _posicionar(self) -> None:
        tela = Gdk.Display.get_default()
        ativa = Gdk.Screen.get_default().get_active_window()
        mon = tela.get_monitor_at_window(ativa) if ativa else None
        if not mon:
            _, x, y = tela.get_default_seat().get_pointer().get_position()[0:3]
            mon = tela.get_monitor_at_point(x, y)
        g, area = mon.get_geometry(), mon.get_workarea()
        if self.topo:  # abaixo do painel, se ele estiver no alto
            self.jan.move(g.x, max(area.y, g.y))
        else:
            base = area.y + area.height if area.y + area.height < g.y + g.height else g.y + g.height  # acima do painel
            self.jan.move(g.x, base - self.altura)
        self.jan.resize(g.width, self.altura)

    def _tique(self) -> bool:
        self.nivel += (self.alvo - self.nivel) * (0.5 if self.alvo > self.nivel else 0.12)
        if self.estado in ("done", "erro") and time.time() - self.t0 > 0.5:
            self.estado = "idle"
        if self.estado == "idle":
            self.jan.hide()
            self.timer = None
            return False
        self.jan.queue_draw()
        return True

    def _desenhar(self, _w, cr) -> bool:
        cr.set_operator(cairo.OPERATOR_SOURCE)
        cr.set_source_rgba(0, 0, 0, 0)
        cr.paint()
        cr.set_operator(cairo.OPERATOR_OVER)
        w, h = self.jan.get_size()
        k, cor = self.k, self.cores
        if self.topo:  # desenha como se fosse embaixo, de ponta-cabeça
            cr.translate(0, h)
            cr.scale(1, -1)
        t, e = time.time() - self.t0, self.estado
        ganho, n, p = 1.0, self.nivel, 1.0
        if e == "rec":
            p = 1 - (1 - min(t / 0.45, 1)) ** 3  # cresce do centro
        elif e == "proc":
            n = 0.25
        elif e in ("done", "erro"):
            ganho, n = max(0.0, 1 - t / 0.5), (0.3 if e == "done" else 0.8)
        c_nuc, c_bri, c_nev = ERRO if e == "erro" else (None, cor["brilho"], cor["nevoa"])
        meia = w / 2 * p

        def horizontal(cor_centro, cor_ponta, alfa: float):
            g = cairo.LinearGradient(w / 2 - meia, 0, w / 2 + meia, 0)
            for i in range(11):  # (1 - fx)^1.8, simétrico a partir do centro
                pos = i / 10
                fx = abs(pos - 0.5) * 2
                a = (1 - fx) ** 1.8 * alfa * ganho
                c = [cp + (cc - cp) * (1 - fx) for cc, cp in zip(cor_centro, cor_ponta)]
                g.add_color_stop_rgba(pos, *c, a)
            return g

        def vertical(alcance: float):
            g = cairo.LinearGradient(0, h, 0, h - alcance)
            for i in range(6):
                g.add_color_stop_rgba(i / 5, 0, 0, 0, np.exp(-i * 1.2))
            return g

        # névoa, brilho e núcleo (2 px)
        cr.set_source(horizontal(c_nev, c_nev, 0.25 * n))
        cr.mask(vertical(h))
        cr.set_source(horizontal(c_bri, c_bri, 0.45 + 0.45 * n))
        cr.mask(vertical((4 + n * 16) * k))
        cr.set_source(horizontal(c_nuc or cor["centro"], c_nuc or cor["ponta"], 1.0))
        cr.rectangle(w / 2 - meia, h - 2 * k, meia * 2, 2 * k)
        cr.fill()
        if e == "proc":  # pulso saindo do centro para as pontas
            onda = (t % 1.0) * w / 2
            for x in (w / 2 - onda, w / 2 + onda):
                g = cairo.RadialGradient(x, h, 0, x, h, 60)
                g.add_color_stop_rgba(0, *cor["centro"], 0.9 * (1 - abs(x - w / 2) / (w / 2)))
                g.add_color_stop_rgba(1, *cor["centro"], 0)
                cr.set_source(g)
                cr.rectangle(x - 60, h - 6 * k, 120, 6 * k)
                cr.fill()
        return False


# ---------- bandeja ----------
class Bandeja:
    def __init__(self, ao_sair, ao_atualizar, versao: str, ao_colar=None) -> None:
        self.ao_sair, self.ao_atualizar, self.versao, self.ao_colar = ao_sair, ao_atualizar, versao, ao_colar
        menu = Gtk.Menu()

        def item(rotulo: str, acao=None, negrito: bool = False, icone: str | None = None) -> None:
            mi = Gtk.MenuItem()
            caixa = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=8)
            if icone:
                caixa.pack_start(Gtk.Image.new_from_icon_name(icone, Gtk.IconSize.MENU), False, False, 0)
            rot = Gtk.Label(xalign=0)
            rot.set_markup(f"<b>{rotulo}</b>" if negrito else GLib.markup_escape_text(rotulo))
            caixa.pack_start(rot, True, True, 0)
            mi.add(caixa)
            if acao:
                mi.connect("activate", lambda *_: acao())
            else:
                mi.set_sensitive(False)
            menu.append(mi)

        item(f"{NOME}   ·   v{versao.split(' ')[0]}", icone="audio-input-microphone")
        item("Segure Ctrl + Super para ditar")
        menu.append(Gtk.SeparatorMenuItem())
        item("Configurações…", lambda: threading.Thread(target=self.abrir_configuracoes, daemon=True).start(), negrito=True, icone="preferences-system")
        item("Colar último ditado", lambda: self.ao_colar and self.ao_colar(), icone="edit-paste")
        item("Procurar atualização",
             lambda: threading.Thread(target=self.ao_atualizar, args=(True,), daemon=True).start(), icone="view-refresh")
        item("Ver log", lambda: abrir(ARQ_LOG), icone="text-x-generic")
        inicio = Gtk.CheckMenuItem(label="Iniciar com a sessão")
        inicio.set_active(inicio_automatico())
        inicio.connect("toggled", lambda w: definir_inicio_automatico(w.get_active()))
        menu.append(inicio)
        menu.append(Gtk.SeparatorMenuItem())
        item(f"Sair do {NOME}", self._sair, icone="system-shutdown")
        menu.show_all()
        self.menu = menu
        dica = f"{NOME} v{versao.split(' ')[0]} · segure Ctrl+Super para ditar"
        try:
            gi.require_version("XApp", "1.0")
            from gi.repository import XApp

            self.icone = XApp.StatusIcon()
            self.icone.set_icon_name(ICONE_PNG)
            self.icone.set_tooltip_text(dica)
            self.icone.set_primary_menu(menu)
            self.icone.set_secondary_menu(menu)
        except (ValueError, ImportError):
            try:
                gi.require_version("AyatanaAppIndicator3", "0.1")
                from gi.repository import AyatanaAppIndicator3 as AI

                self.icone = AI.Indicator.new("renantalk", ICONE_PNG, AI.IndicatorCategory.APPLICATION_STATUS)
                self.icone.set_status(AI.IndicatorStatus.ACTIVE)
                self.icone.set_title(dica)
                self.icone.set_menu(menu)
            except (ValueError, ImportError):
                self.icone = Gtk.StatusIcon.new_from_file(ICONE_PNG)
                self.icone.set_tooltip_text(dica)
                self.icone.connect("popup-menu", lambda i, b, t: menu.popup(None, None, None, None, b, t))
                self.icone.connect("activate", lambda i: menu.popup(None, None, None, None, 1, Gtk.get_current_event_time()))

    def _sair(self) -> None:
        self.remover()
        self.ao_sair()

    def notificar(self, texto: str) -> None:
        if not ajustes.ler()["notificacoes"]:  # desligadas: erro continua no bipe grave e no log
            return
        subprocess.Popen(["notify-send", "-i", ICONE_PNG, "-a", NOME, NOME, texto],
                         stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

    def remover(self) -> None:
        try:
            self.icone.set_visible(False)
        except Exception:  # noqa: BLE001
            pass

    def abrir_configuracoes(self, aba: str = "inicio") -> None:
        abrir_configuracoes(aba)
