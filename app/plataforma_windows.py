"""Peças do RenanTalk específicas do Windows: atalho, colar, bipe, instância única, luz e bandeja."""
import ctypes
import time

import keyboard
import pyperclip

import ajustes
from bandeja import Bandeja  # noqa: F401  (reexportado para o ditado)
from luz import Luz  # noqa: F401

_mutex = None


def bipe(freq: int, ms: int) -> None:
    ajustes.bipe(freq, ms)  # tom gerado, com o volume dos ajustes (winsound.Beep não tem volume)


def colar(texto: str) -> None:
    anterior = pyperclip.paste()
    pyperclip.copy(texto)
    time.sleep(0.05)
    keyboard.send("ctrl+v")
    time.sleep(0.3)
    pyperclip.copy(anterior)  # devolve o que estava na área de transferência


def _mascarar_win() -> None:
    # Toque numa tecla sem função (VK 0xE8) para o Windows não abrir o menu Iniciar ao soltar o Win.
    ctypes.windll.user32.keybd_event(0xE8, 0, 0, 0)
    ctypes.windll.user32.keybd_event(0xE8, 0, 2, 0)


_NOMES = {"ctrl": "ctrl", "left ctrl": "ctrl", "right ctrl": "ctrl", "alt": "alt", "left alt": "alt",
          "right alt": "alt", "alt gr": "alt", "shift": "shift", "left shift": "shift", "right shift": "shift",
          "windows": "win", "left windows": "win", "right windows": "win"}


def rodar(ao_iniciar, ao_parar, ao_outra, esta_gravando) -> None:
    """Escuta o atalho de ditar (bloqueia a thread principal). O atalho é relido a cada tecla."""
    casador = ajustes.Casador()

    def evento(ev: keyboard.KeyboardEvent) -> None:
        bruto = (ev.name or "").lower()
        nome = _NOMES.get(bruto, bruto)
        descer = ev.event_type == "down"
        alvo = ajustes.validar_atalho(ajustes.ler()["atalho_ditar"])
        if descer and not esta_gravando():  # solta teclas "presas" (Win+L, etc. não entregam o key-up)
            for m in list(casador.apertadas):
                if m in ajustes.MODIFICADORES and not keyboard.is_pressed("windows" if m == "win" else m):
                    casador.apertadas.discard(m)
        r = casador.evento(nome, descer, alvo, esta_gravando())
        if r == "iniciar":
            ao_iniciar()
        elif r == "parar":
            ao_parar()
            _mascarar_win()
        elif r == "outra":
            ao_outra(f"{ev.name!r} scan={ev.scan_code}")  # Ctrl+Win+outra tecla = atalho do Windows

    keyboard.hook(evento)
    keyboard.wait()


def instancia_unica() -> bool:
    global _mutex
    k32 = ctypes.WinDLL("kernel32", use_last_error=True)
    _mutex = k32.CreateMutexW(None, False, "RenanTalk")
    return ctypes.get_last_error() != 183  # ERROR_ALREADY_EXISTS


def liberar_instancia() -> None:
    ctypes.windll.kernel32.CloseHandle(_mutex)  # libera o "já está rodando" para a nova instância
