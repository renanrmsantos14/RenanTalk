"""Peças do RenanTalk específicas do Windows: atalho, colar, bipe, instância única, luz e bandeja."""
import ctypes
import time
import winsound

import keyboard
import pyperclip

from bandeja import Bandeja  # noqa: F401  (reexportado para o ditado)
from luz import Luz  # noqa: F401

_mutex = None


def bipe(freq: int, ms: int) -> None:
    winsound.Beep(freq, ms)


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


def rodar(ao_iniciar, ao_parar, ao_outra, esta_gravando) -> None:
    """Escuta Ctrl+Win (bloqueia a thread principal)."""
    def evento(ev: keyboard.KeyboardEvent) -> None:
        nome = (ev.name or "").lower()
        eh_ctrl = "ctrl" in nome
        eh_win = nome in ("windows", "left windows", "right windows")
        if ev.event_type == "down":
            if esta_gravando() and not (eh_ctrl or eh_win):
                ao_outra(f"{ev.name!r} scan={ev.scan_code}")  # Ctrl+Win+outra tecla = atalho do Windows
            elif not esta_gravando() and (eh_ctrl or eh_win) and keyboard.is_pressed("ctrl") and (
                keyboard.is_pressed("windows") or keyboard.is_pressed("left windows") or eh_win
            ):
                ao_iniciar()
        elif ev.event_type == "up" and esta_gravando() and (eh_ctrl or eh_win):
            ao_parar()
            _mascarar_win()

    keyboard.hook(evento)
    keyboard.wait()


def instancia_unica() -> bool:
    global _mutex
    k32 = ctypes.WinDLL("kernel32", use_last_error=True)
    _mutex = k32.CreateMutexW(None, False, "RenanTalk")
    return ctypes.get_last_error() != 183  # ERROR_ALREADY_EXISTS


def liberar_instancia() -> None:
    ctypes.windll.kernel32.CloseHandle(_mutex)  # libera o "já está rodando" para a nova instância
