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


def app_ativo() -> str:
    """Nome do executável da janela em primeiro plano (ex.: 'code'); vazio se não der para saber."""
    try:
        from ctypes import wintypes

        u, k = ctypes.windll.user32, ctypes.windll.kernel32
        k.OpenProcess.restype = wintypes.HANDLE
        pid = wintypes.DWORD()
        u.GetWindowThreadProcessId(u.GetForegroundWindow(), ctypes.byref(pid))
        h = k.OpenProcess(0x1000, False, pid.value)  # PROCESS_QUERY_LIMITED_INFORMATION
        if not h:
            return ""
        try:
            buf = ctypes.create_unicode_buffer(1024)
            n = wintypes.DWORD(len(buf))
            if not k.QueryFullProcessImageNameW(h, 0, buf, ctypes.byref(n)):
                return ""
            nome = buf.value.replace("/", "\\").rsplit("\\", 1)[-1]
            return nome[:-4] if nome.lower().endswith(".exe") else nome
        finally:
            k.CloseHandle(h)
    except Exception:  # noqa: BLE001
        return ""


def _mascarar_win() -> None:
    # Toque numa tecla sem função (VK 0xE8) para o Windows não abrir o menu Iniciar ao soltar o Win.
    ctypes.windll.user32.keybd_event(0xE8, 0, 0, 0)
    ctypes.windll.user32.keybd_event(0xE8, 0, 2, 0)


_NOMES = {"ctrl": "ctrl", "left ctrl": "ctrl", "right ctrl": "ctrl", "alt": "alt", "left alt": "alt",
          "right alt": "alt", "alt gr": "alt", "shift": "shift", "left shift": "shift", "right shift": "shift",
          "windows": "win", "left windows": "win", "right windows": "win"}


def rodar(ao_iniciar, ao_parar, ao_outra, esta_gravando, ao_colar=None) -> None:
    """Escuta os atalhos de ditar e de colar o último (bloqueia a thread principal). Relidos a cada tecla."""
    casador = ajustes.Casador()

    def evento(ev: keyboard.KeyboardEvent) -> None:
        bruto = (ev.name or "").lower()
        nome = _NOMES.get(bruto, bruto)
        descer = ev.event_type == "down"
        a = ajustes.ler()
        alvo, alvo_colar = ajustes.validar_atalho(a["atalho_ditar"]), ajustes.validar_atalho(a["atalho_colar"])
        if descer and not esta_gravando():  # solta teclas "presas" (Win+L, etc. não entregam o key-up)
            for m in list(casador.apertadas):
                if m in ajustes.MODIFICADORES and not keyboard.is_pressed("windows" if m == "win" else m):
                    casador.apertadas.discard(m)
        r = casador.evento(nome, descer, alvo, esta_gravando(), alvo_colar)
        if r == "colar" and ao_colar:
            ajustes.colar_ao_soltar(casador, ao_colar)
        elif r == "iniciar":
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
