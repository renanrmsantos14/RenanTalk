"""Peças do RenanTalk específicas do Windows: atalho, colar, bipe, instância única, luz e bandeja."""
import ctypes
import os
import time

import keyboard
import pyperclip

import ajustes
from comum import log
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
        # Shift+1 chega como "!": a fileira de números (scan 2–11) vira o dígito; o teclado numérico não entra
        if 2 <= (ev.scan_code or 0) <= 11 and not ev.is_keypad:
            bruto = "1234567890"[ev.scan_code - 2]
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


def abaixar_outros(fator: float) -> list:
    """Põe cada app que toca som em `fator` do volume atual. Devolve o que mudou, para restaurar_outros.

    Chamada sempre da mesma thread (a fila de volume do ditado.pyw): as sessões são objetos COM dela."""
    feitos = []
    try:
        import comtypes
        from pycaw.pycaw import AudioUtilities

        comtypes.CoInitialize()  # a thread da fila de volume não tem COM iniciado
        sessoes = AudioUtilities.GetAllSessions()
    except Exception as e:  # noqa: BLE001 - COM levanta tipos variados; o ditado não pode parar por isso
        log(f"abaixar volume: {e!r}")
        return feitos
    for s in sessoes:  # uma sessão que sumiu não impede abaixar as outras
        try:
            if s.ProcessId in (0, os.getpid()):  # 0 = sons do Windows; o nosso = os bipes, no volume dos ajustes
                continue
            v = s.SimpleAudioVolume
            antes = v.GetMasterVolume()
            v.SetMasterVolume(antes * fator, None)
            feitos.append((v, antes))
        except Exception as e:  # noqa: BLE001
            log(f"abaixar volume: {e!r}")
    return feitos


def restaurar_outros(feitos: list) -> None:
    """Volta sempre o volume de antes (como no Linux)."""
    for v, antes in feitos:
        try:
            v.SetMasterVolume(antes, None)
        except Exception as e:  # noqa: BLE001 - o app fechou durante o ditado
            log(f"restaurar volume: {e!r}")


def instancia_unica() -> bool:
    global _mutex
    k32 = ctypes.WinDLL("kernel32", use_last_error=True)
    _mutex = k32.CreateMutexW(None, False, "RenanTalk")
    return ctypes.get_last_error() != 183  # ERROR_ALREADY_EXISTS


def liberar_instancia() -> None:
    ctypes.windll.kernel32.CloseHandle(_mutex)  # libera o "já está rodando" para a nova instância
