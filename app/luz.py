"""Linha de luz azul na borda inferior do monitor ativo (indicador do ditado).

Janela em camadas (per-pixel alpha), sempre por cima, sem foco e sem capturar clique.
Estados: idle (oculta), rec (cresce do centro, brilho segue a voz),
proc (pulso saindo do centro), done/erro (apaga em 0,5 s).
"""
import ctypes
import threading
import time
from ctypes import wintypes as W

import numpy as np

import ajustes

u32 = ctypes.windll.user32
g32 = ctypes.windll.gdi32
k32 = ctypes.windll.kernel32
try:  # coordenadas em pixels reais em telas com escala
    u32.SetProcessDpiAwarenessContext(ctypes.c_void_p(-4))
except Exception:  # noqa: BLE001
    pass

ALTURA = 36  # px da faixa desenhada (núcleo + brilho)
NUCLEO_PX = 2
FPS = 40

# Cores RGB (a cor da luz vem de ajustes.CORES_LUZ; o erro é sempre vermelho)
ERRO = (np.array([254, 202, 202], np.float32), np.array([248, 113, 113], np.float32), np.array([220, 38, 38], np.float32))

LRESULT = ctypes.c_ssize_t
WNDPROC = ctypes.WINFUNCTYPE(LRESULT, W.HWND, W.UINT, W.WPARAM, W.LPARAM)


class WNDCLASSW(ctypes.Structure):
    _fields_ = [("style", W.UINT), ("lpfnWndProc", WNDPROC), ("cbClsExtra", ctypes.c_int), ("cbWndExtra", ctypes.c_int),
                ("hInstance", W.HINSTANCE), ("hIcon", W.HICON), ("hCursor", W.HANDLE), ("hbrBackground", W.HBRUSH),
                ("lpszMenuName", W.LPCWSTR), ("lpszClassName", W.LPCWSTR)]


class MONITORINFO(ctypes.Structure):
    _fields_ = [("cbSize", W.DWORD), ("rcMonitor", W.RECT), ("rcWork", W.RECT), ("dwFlags", W.DWORD)]


class BLEND(ctypes.Structure):
    _fields_ = [("op", ctypes.c_ubyte), ("flags", ctypes.c_ubyte), ("alpha", ctypes.c_ubyte), ("fmt", ctypes.c_ubyte)]


class BMIH(ctypes.Structure):
    _fields_ = [("biSize", W.DWORD), ("biWidth", W.LONG), ("biHeight", W.LONG), ("biPlanes", W.WORD), ("biBitCount", W.WORD),
                ("biCompression", W.DWORD), ("biSizeImage", W.DWORD), ("biXPelsPerMeter", W.LONG), ("biYPelsPerMeter", W.LONG),
                ("biClrUsed", W.DWORD), ("biClrImportant", W.DWORD)]


u32.DefWindowProcW.argtypes = [W.HWND, W.UINT, W.WPARAM, W.LPARAM]
u32.DefWindowProcW.restype = LRESULT
u32.CreateWindowExW.argtypes = [W.DWORD, W.LPCWSTR, W.LPCWSTR, W.DWORD, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_int,
                                W.HWND, W.HMENU, W.HINSTANCE, W.LPVOID]
u32.CreateWindowExW.restype = W.HWND
u32.UpdateLayeredWindow.argtypes = [W.HWND, W.HDC, ctypes.POINTER(W.POINT), ctypes.POINTER(W.SIZE), W.HDC,
                                    ctypes.POINTER(W.POINT), W.COLORREF, ctypes.POINTER(BLEND), W.DWORD]
u32.GetDC.argtypes = [W.HWND]
u32.GetDC.restype = W.HDC
u32.MonitorFromWindow.argtypes = [W.HWND, W.DWORD]
u32.MonitorFromWindow.restype = W.HMONITOR
u32.GetMonitorInfoW.argtypes = [W.HMONITOR, ctypes.c_void_p]
u32.GetForegroundWindow.restype = W.HWND
u32.ShowWindow.argtypes = [W.HWND, ctypes.c_int]
u32.SetWindowPos.argtypes = [W.HWND, W.HWND, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_int, W.UINT]
u32.PeekMessageW.argtypes = [ctypes.POINTER(W.MSG), W.HWND, W.UINT, W.UINT, W.UINT]
u32.TranslateMessage.argtypes = [ctypes.POINTER(W.MSG)]
u32.DispatchMessageW.argtypes = [ctypes.POINTER(W.MSG)]
g32.CreateCompatibleDC.argtypes = [W.HDC]
g32.CreateCompatibleDC.restype = W.HDC
g32.CreateDIBSection.argtypes = [W.HDC, ctypes.c_void_p, W.UINT, ctypes.POINTER(ctypes.c_void_p), W.HANDLE, W.DWORD]
g32.CreateDIBSection.restype = W.HBITMAP
g32.SelectObject.argtypes = [W.HDC, W.HGDIOBJ]
g32.SelectObject.restype = W.HGDIOBJ
g32.DeleteObject.argtypes = [W.HGDIOBJ]
k32.GetModuleHandleW.restype = W.HMODULE


class Luz:
    def __init__(self) -> None:
        self.estado = "idle"
        self.ligada = True
        self.cfg = ajustes.ler()["luz"]  # trocada em definir("rec") e aplicada só no laço (sem corrida com o desenho)
        self.t0 = time.time()
        self.alvo = 0.0
        self.nivel = 0.0
        self.reposicionar = False
        self.evento = threading.Event()
        self.w = 0
        self.hbmp = None
        self.altura = ALTURA
        self._aplicar_ajustes()
        threading.Thread(target=self._loop, daemon=True).start()

    # API usada pelo ditado
    def definir(self, estado: str) -> None:
        if estado == "rec":
            self.cfg = ajustes.ler()["luz"]  # vale a partir deste ditado
            self.ligada = self.cfg["ligada"]
        if not self.ligada:  # luz desligada: não mostra a janela (o bipe e o resto do ditado seguem normais)
            return
        if estado == "rec":
            self.reposicionar = True
            self.nivel = 0.0
        self.estado = estado
        self.t0 = time.time()
        self.evento.set()

    def audio(self, v: float) -> None:
        self.alvo = v

    # interno
    def _aplicar_ajustes(self) -> None:
        """Cor, espessura e posição do ditado atual (só roda no laço da luz)."""
        cfg = self.cfg
        self.topo = cfg["posicao"] == "topo"
        self.k = ajustes.ESPESSURAS[cfg["espessura"]]
        self.paleta = {n: np.array(rgb, np.float32) for n, rgb in ajustes.CORES_LUZ[cfg["cor"]].items()}
        altura = round(ALTURA * self.k)
        d = np.arange(altura, dtype=np.float32)  # distância até a borda; linha 0 = topo
        self.altura = altura
        self.d = (d if self.topo else d[::-1])[:, None]
        self.nucleo = np.clip(NUCLEO_PX * self.k - self.d, 0, 1)  # núcleo cheio, borda suave
        self.w = 0  # força recriar o bitmap e as curvas na próxima posição

    def _loop(self) -> None:
        self._proc = WNDPROC(lambda h, m, w, l: u32.DefWindowProcW(h, m, w, l))
        inst = k32.GetModuleHandleW(None)
        wc = WNDCLASSW(lpfnWndProc=self._proc, hInstance=inst, lpszClassName="LuzDitado")
        u32.RegisterClassW(ctypes.byref(wc))
        ex = 0x80000 | 0x20 | 0x8 | 0x80 | 0x8000000  # LAYERED | TRANSPARENT | TOPMOST | TOOLWINDOW | NOACTIVATE
        self.hwnd = u32.CreateWindowExW(ex, "LuzDitado", "", 0x80000000, 0, 0, 1, 1, None, None, inst, None)
        self.tela = u32.GetDC(None)
        self.mem = g32.CreateCompatibleDC(self.tela)
        msg = W.MSG()
        quadro = 0
        while True:
            if self.estado == "idle":
                u32.ShowWindow(self.hwnd, 0)
                self.evento.wait()
                self.evento.clear()
                continue
            if self.reposicionar:
                self.reposicionar = False
                self._aplicar_ajustes()
                self._posicionar()
            while u32.PeekMessageW(ctypes.byref(msg), None, 0, 0, 1):
                u32.TranslateMessage(ctypes.byref(msg))
                u32.DispatchMessageW(ctypes.byref(msg))
            self._desenhar()
            quadro += 1
            if quadro % (FPS // 4) == 0:  # a barra de tarefas também é "sempre no topo": reafirma
                self._topo()
            time.sleep(1 / FPS)

    def _posicionar(self) -> None:
        mi = MONITORINFO()
        mi.cbSize = ctypes.sizeof(mi)
        u32.GetMonitorInfoW(u32.MonitorFromWindow(u32.GetForegroundWindow(), 2), ctypes.byref(mi))
        r, t = mi.rcMonitor, mi.rcWork
        if self.topo:  # barra de tarefas no alto: a linha encosta na base dela
            self.x, self.y = r.left, max(t.top, r.top)
        else:
            base = t.bottom if t.bottom < r.bottom else r.bottom  # barra embaixo: linha encosta no topo dela
            self.x, self.y = r.left, base - self.altura
        w = r.right - r.left
        if w != self.w:
            bi = BMIH(biSize=ctypes.sizeof(BMIH), biWidth=w, biHeight=-self.altura, biPlanes=1, biBitCount=32)
            bits = ctypes.c_void_p()
            hbmp = g32.CreateDIBSection(self.mem, ctypes.byref(bi), 0, ctypes.byref(bits), None, 0)
            g32.SelectObject(self.mem, hbmp)
            if self.hbmp:
                g32.DeleteObject(self.hbmp)
            self.hbmp, self.bits, self.w = hbmp, bits, w
            fx = np.abs(np.arange(w, dtype=np.float32) - (w - 1) / 2) / ((w - 1) / 2)
            self.fx = fx  # 0 no centro, 1 nas bordas
            self.queda = (1 - fx) ** 1.8  # forte no centro, apaga nas bordas
            ponta, centro = self.paleta["ponta"], self.paleta["centro"]
            self.cor_nucleo = ponta[:, None] + (centro - ponta)[:, None] * (1 - fx)  # 3 x w
        self._topo()

    def _topo(self) -> None:
        u32.SetWindowPos(self.hwnd, W.HWND(-1), 0, 0, 0, 0, 0x10 | 0x2 | 0x1 | 0x40)  # topo, sem foco, mostrar

    def _desenhar(self) -> None:
        if not self.w:
            return
        t = time.time() - self.t0
        e = self.estado
        self.nivel += (self.alvo - self.nivel) * (0.5 if self.alvo > self.nivel else 0.12)
        fx, ganho = self.fx, 1.0
        if e == "rec":
            p = 1 - (1 - min(t / 0.45, 1)) ** 3  # cresce do centro com desaceleração
            H = np.clip((p - fx) / 0.06, 0, 1) * self.queda
            n = self.nivel
        elif e == "proc":
            onda = t % 1.0  # pulso saindo do centro para as bordas
            H = self.queda * (0.45 + 0.55 * np.exp(-((fx - onda) ** 2) / 0.004))
            n = 0.25
        else:  # done / erro: apaga
            ganho = max(0.0, 1 - t / 0.5)
            if ganho == 0:
                self.estado = "idle"
                return
            H, n = self.queda, (0.3 if e == "done" else 0.8)
        nucleo_c, brilho_c, nevoa_c = (ERRO if e == "erro" else (None, self.paleta["brilho"], self.paleta["nevoa"]))
        brilho = np.exp(-self.d / ((1.5 + n * 6) * self.k)) * (0.45 + 0.45 * n)
        nevoa = np.exp(-self.d / (14 * self.k)) * 0.25 * n
        Ac, Ab, An = self.nucleo * H, brilho * H, nevoa * H
        A = np.clip(Ac + Ab + An, 0, 1) * ganho
        cn = self.cor_nucleo if nucleo_c is None else nucleo_c[:, None] * np.ones_like(fx)
        img = np.empty((self.altura, self.w, 4), np.uint8)
        for i, canal in enumerate((2, 1, 0)):  # BGRA, cor pré-multiplicada
            c = (Ac * cn[canal] + Ab * brilho_c[canal] + An * nevoa_c[canal]) * ganho
            img[..., i] = np.minimum(c, A * 255)
        img[..., 3] = A * 255
        ctypes.memmove(self.bits, img.ctypes.data, img.nbytes)
        u32.UpdateLayeredWindow(self.hwnd, self.tela, ctypes.byref(W.POINT(self.x, self.y)),
                                ctypes.byref(W.SIZE(self.w, self.altura)), self.mem, ctypes.byref(W.POINT(0, 0)), 0,
                                ctypes.byref(BLEND(0, 0, 255, 1)), 2)
