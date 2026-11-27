"""Peças do RenanTalk específicas do Windows: atalho, colar, bipe, instância única, luz e bandeja."""
import ctypes
import os
import time

import keyboard

import ajustes
from comum import log
from bandeja import Bandeja  # noqa: F401  (reexportado para o ditado)
from luz import Luz  # noqa: F401

_mutex = None


def bipe(freq: int, ms: int) -> None:
    ajustes.bipe(freq, ms)  # tom gerado, com o volume dos ajustes (winsound.Beep não tem volume)


# ---------- colar sem deixar rastro na área de transferência ----------
_u32 = ctypes.WinDLL("user32", use_last_error=True)
_k32 = ctypes.WinDLL("kernel32", use_last_error=True)
_H = ctypes.c_void_p
for _f, _args, _ret in (
    (_u32.OpenClipboard, [_H], ctypes.c_int), (_u32.CloseClipboard, [], ctypes.c_int),
    (_u32.EmptyClipboard, [], ctypes.c_int), (_u32.EnumClipboardFormats, [ctypes.c_uint], ctypes.c_uint),
    (_u32.GetClipboardData, [ctypes.c_uint], _H), (_u32.SetClipboardData, [ctypes.c_uint, _H], _H),
    (_u32.RegisterClipboardFormatW, [ctypes.c_wchar_p], ctypes.c_uint),
    (_k32.GlobalAlloc, [ctypes.c_uint, ctypes.c_size_t], _H), (_k32.GlobalFree, [_H], _H),
    (_k32.GlobalLock, [_H], _H), (_k32.GlobalUnlock, [_H], ctypes.c_int), (_k32.GlobalSize, [_H], ctypes.c_size_t),
):
    _f.argtypes, _f.restype = _args, _ret
CF_UNICODETEXT = 13
# CF_BITMAP, METAFILEPICT, PALETTE, ENHMETAFILE e os de exibição guardam handles GDI, não memória: o Windows
# refaz o CF_BITMAP a partir do CF_DIB, que é copiado
_NAO_MEMORIA = {2, 3, 9, 14, 0x80, 0x82, 0x83, 0x8E}
# marcas que tiram o conteúdo do histórico (Win+V) e da sincronização na nuvem
_SEM_HISTORICO = [(_u32.RegisterClipboardFormatW(n), b"\0\0\0\0") for n in (
    "ExcludeClipboardContentFromMonitorProcessing", "CanIncludeInClipboardHistory", "CanUploadToCloudClipboard")]


def _abrir_area() -> bool:
    for _ in range(50):  # outro app pode estar com ela aberta
        if _u32.OpenClipboard(None):
            return True
        time.sleep(0.01)
    return False


def _ler_area() -> list[tuple[int, bytes]]:
    itens, f = [], 0
    while f := _u32.EnumClipboardFormats(f):
        h = _u32.GetClipboardData(f) if f not in _NAO_MEMORIA else None
        p = _k32.GlobalLock(h) if h else None
        if p:
            itens.append((f, ctypes.string_at(p, _k32.GlobalSize(h))))
            _k32.GlobalUnlock(h)
    return itens


def _gravar_area(itens: list[tuple[int, bytes]]) -> None:
    _u32.EmptyClipboard()
    for f, dados in itens:
        h = _k32.GlobalAlloc(0x0002, max(len(dados), 1))  # GMEM_MOVEABLE; o Windows passa a ser o dono
        ctypes.memmove(_k32.GlobalLock(h), dados, len(dados))
        _k32.GlobalUnlock(h)
        if not _u32.SetClipboardData(f, h):
            _k32.GlobalFree(h)


def colar(texto: str) -> None:
    """Cola pelo Ctrl+V e devolve a área de transferência como estava, sem o ditado no histórico."""
    if not _abrir_area():
        log("colar: área de transferência ocupada por outro app")
        return
    try:
        anterior = _ler_area()
        _gravar_area([(CF_UNICODETEXT, (texto + "\0").encode("utf-16-le")), *_SEM_HISTORICO])
    finally:
        _u32.CloseClipboard()
    time.sleep(0.05)
    keyboard.send("ctrl+v")
    time.sleep(0.3)
    if _abrir_area():
        try:  # devolvido fora do histórico também, senão o item anterior aparece duplicado no Win+V
            _gravar_area([*anterior, *_SEM_HISTORICO] if anterior else [])
        finally:
            _u32.CloseClipboard()


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
    """Escuta os atalhos de ditar, traduzir e colar o último (bloqueia a thread principal). Relidos a cada tecla."""
    casador = ajustes.Casador()

    def evento(ev: keyboard.KeyboardEvent) -> None:
        bruto = (ev.name or "").lower()
        # Shift+1 chega como "!": a fileira de números (scan 2–11) vira o dígito; o teclado numérico não entra
        if 2 <= (ev.scan_code or 0) <= 11 and not ev.is_keypad:
            bruto = "1234567890"[ev.scan_code - 2]
        nome = _NOMES.get(bruto, bruto)
        descer = ev.event_type == "down"
        a = ajustes.ler()
        ditar, colar, traduzir = (ajustes.validar_atalho(a[c]) for c in ajustes.ATALHOS)
        if descer and not esta_gravando():  # solta teclas "presas" (Win+L, etc. não entregam o key-up)
            for m in list(casador.apertadas):
                if m in ajustes.MODIFICADORES and not keyboard.is_pressed("windows" if m == "win" else m):
                    casador.apertadas.discard(m)
        r = casador.evento(nome, descer, ditar, esta_gravando(), colar, traduzir)
        if r == "colar" and ao_colar:
            ajustes.colar_ao_soltar(casador, ao_colar)
        elif r in ("iniciar", "traduzir"):
            ao_iniciar(r == "traduzir")
        elif r == "parar":
            ao_parar()
            _mascarar_win()
        elif r == "outra":
            ao_outra(f"{ev.name!r} scan={ev.scan_code}")  # Ctrl+Win+outra tecla = atalho do Windows

    keyboard.hook(evento)
    keyboard.wait()


# O Windows guarda o volume por app: um app que fechou ainda baixo voltaria baixo. Fica aqui até reaparecer.
_pendentes: dict[str, float] = {}
_EXPIRADA = 2  # AudioSessionState: o app fechou


def _sessoes() -> list:
    import comtypes
    from pycaw.pycaw import AudioUtilities

    comtypes.CoInitialize()  # a thread da fila de volume não tem COM iniciado
    # 0 = sons do Windows; o nosso = os bipes, no volume dos ajustes
    return [s for s in AudioUtilities.GetAllSessions() if s.ProcessId not in (0, os.getpid())]


def abaixar_outros(fator: float) -> list:
    """Põe cada app com som em `fator` do volume atual. Devolve o que mudou, para restaurar_outros.

    Chamada sempre da mesma thread (a fila de volume do ditado.pyw): as sessões são objetos COM dela."""
    feitos = []
    try:
        sessoes = _sessoes()
    except Exception as e:  # noqa: BLE001 - COM levanta tipos variados; o ditado não pode parar por isso
        log(f"abaixar volume: {e!r}")
        return feitos
    for s in sessoes:  # uma sessão com erro não impede abaixar as outras
        try:
            v = s.SimpleAudioVolume
            antes = _pendentes.get(s.Identifier, v.GetMasterVolume())
            v.SetMasterVolume(antes * fator, None)
            _pendentes.pop(s.Identifier, None)
            feitos.append((s.Identifier, antes))
        except Exception as e:  # noqa: BLE001
            log(f"abaixar volume: {e!r}")
    return feitos


def restaurar_outros(feitos: list) -> None:
    """Volta sempre o volume de antes (como no Linux), pelas sessões de agora: o app que fechou e abriu de novo
    tem sessão nova. O que não está aberto fica pendente para o próximo ditado."""
    falta = dict(feitos)
    try:
        sessoes = _sessoes()
    except Exception as e:  # noqa: BLE001
        log(f"restaurar volume: {e!r}")
        sessoes = []
    voltaram = set()
    for s in sessoes:
        try:
            if s.Identifier in falta and s.State != _EXPIRADA:
                s.SimpleAudioVolume.SetMasterVolume(falta[s.Identifier], None)
                voltaram.add(s.Identifier)
        except Exception as e:  # noqa: BLE001
            log(f"restaurar volume: {e!r}")
    _pendentes.update({k: v for k, v in falta.items() if k not in voltaram})


def instancia_unica() -> bool:
    global _mutex
    k32 = ctypes.WinDLL("kernel32", use_last_error=True)
    _mutex = k32.CreateMutexW(None, False, "RenanTalk")
    return ctypes.get_last_error() != 183  # ERROR_ALREADY_EXISTS


def liberar_instancia() -> None:
    ctypes.windll.kernel32.CloseHandle(_mutex)  # libera o "já está rodando" para a nova instância
