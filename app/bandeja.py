"""Ícone do RenanTalk na área de notificação (perto do relógio), com menu e janela da chave Groq."""
import ctypes
import json
import os
import struct
import threading
import urllib.error
import urllib.request
import zlib
from ctypes import wintypes as W

import numpy as np

from luz import WNDCLASSW, WNDPROC, k32, u32

NOME = "RenanTalk"
PASTA = os.path.join(os.environ["APPDATA"], "RenanTalk")
ANTIGA = os.path.join(os.environ["APPDATA"], "GroqDitado")  # pasta da primeira versão
ARQ_CHAVE = os.path.join(PASTA, "chave.txt")
ARQ_LOG = os.path.join(PASTA, "ditado.log")
ICONE = os.path.join(PASTA, "renantalk.ico")
WM_TRAY = 0x8000 + 1
ITEM_CHAVE, ITEM_LOG, ITEM_ATUALIZAR, ITEM_SAIR = 1, 2, 3, 4

os.makedirs(PASTA, exist_ok=True)
if not os.path.exists(ARQ_CHAVE) and os.path.exists(os.path.join(ANTIGA, "chave.txt")):
    import shutil

    shutil.copy2(os.path.join(ANTIGA, "chave.txt"), ARQ_CHAVE)

s32 = ctypes.windll.shell32


class GUID(ctypes.Structure):
    _fields_ = [("a", W.DWORD), ("b", W.WORD), ("c", W.WORD), ("d", ctypes.c_ubyte * 8)]


class NOTIFYICONDATAW(ctypes.Structure):
    _fields_ = [("cbSize", W.DWORD), ("hWnd", W.HWND), ("uID", W.UINT), ("uFlags", W.UINT), ("uCallbackMessage", W.UINT),
                ("hIcon", W.HICON), ("szTip", W.WCHAR * 128), ("dwState", W.DWORD), ("dwStateMask", W.DWORD),
                ("szInfo", W.WCHAR * 256), ("uVersion", W.UINT), ("szInfoTitle", W.WCHAR * 64), ("dwInfoFlags", W.DWORD),
                ("guidItem", GUID), ("hBalloonIcon", W.HICON)]


s32.Shell_NotifyIconW.argtypes = [W.DWORD, ctypes.POINTER(NOTIFYICONDATAW)]
u32.LoadImageW.argtypes = [W.HINSTANCE, W.LPCWSTR, W.UINT, ctypes.c_int, ctypes.c_int, W.UINT]
u32.LoadImageW.restype = W.HANDLE
u32.CreatePopupMenu.restype = W.HMENU
u32.AppendMenuW.argtypes = [W.HMENU, W.UINT, ctypes.c_size_t, W.LPCWSTR]
u32.TrackPopupMenu.argtypes = [W.HMENU, W.UINT, ctypes.c_int, ctypes.c_int, ctypes.c_int, W.HWND, ctypes.c_void_p]
u32.DestroyMenu.argtypes = [W.HMENU]
u32.GetCursorPos.argtypes = [ctypes.POINTER(W.POINT)]
u32.SetForegroundWindow.argtypes = [W.HWND]
u32.PostMessageW.argtypes = [W.HWND, W.UINT, W.WPARAM, W.LPARAM]
u32.RegisterWindowMessageW.argtypes = [W.LPCWSTR]
u32.RegisterWindowMessageW.restype = W.UINT
u32.GetMessageW.argtypes = [ctypes.POINTER(W.MSG), W.HWND, W.UINT, W.UINT]
u32.TranslateMessage.argtypes = [ctypes.POINTER(W.MSG)]
u32.DispatchMessageW.argtypes = [ctypes.POINTER(W.MSG)]


# ---------- ícone (gerado uma vez: quadrado escuro com a linha de luz azul) ----------
def _png_rgba(a: np.ndarray) -> bytes:
    h, w = a.shape[:2]
    raw = b"".join(b"\x00" + a[y].tobytes() for y in range(h))
    ch = lambda t, d: struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)  # noqa: E731
    return (b"\x89PNG\r\n\x1a\n" + ch(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0))
            + ch(b"IDAT", zlib.compress(raw, 9)) + ch(b"IEND", b""))


def _desenho(n: int) -> np.ndarray:
    s = 4
    N = n * s
    y, x = np.mgrid[0:N, 0:N].astype(np.float32) + 0.5
    m, r = N * 0.03, N * 0.24
    dx = np.maximum(np.abs(x - N / 2) - (N / 2 - m - r), 0)
    dy = np.maximum(np.abs(y - N / 2) - (N / 2 - m - r), 0)
    fundo = (np.hypot(dx, dy) <= r).astype(np.float32)
    fx = np.abs(x - N / 2) / (N * 0.40)
    queda = np.clip(1 - fx, 0, 1) ** 1.6
    nucleo = (np.abs(y - N * 0.60) < N * 0.06) * queda
    brilho = np.exp(-np.abs(y - N * 0.60) / (N * 0.09)) * queda * 0.8
    cor = np.zeros((N, N, 3), np.float32) + np.array([15, 23, 42], np.float32)
    cor += nucleo[..., None] * np.array([150, 195, 255], np.float32) + brilho[..., None] * np.array([45, 110, 255], np.float32)
    img = np.concatenate([np.clip(cor, 0, 255), fundo[..., None] * 255], -1)
    img = img.reshape(n, s, n, s, 4).mean((1, 3))
    return img.round().astype(np.uint8)


def gerar_icone() -> None:
    if os.path.exists(ICONE):
        return
    tamanhos = [16, 20, 24, 32, 48, 64]
    pngs = [_png_rgba(_desenho(t)) for t in tamanhos]
    off = 6 + 16 * len(tamanhos)
    cab = struct.pack("<HHH", 0, 1, len(tamanhos))
    ent = b""
    for t, p in zip(tamanhos, pngs):
        ent += struct.pack("<BBBBHHII", t % 256, t % 256, 0, 0, 1, 32, len(p), off)
        off += len(p)
    with open(ICONE, "wb") as f:
        f.write(cab + ent + b"".join(pngs))


# ---------- chave ----------
def ler_chave() -> str:
    try:
        with open(ARQ_CHAVE, encoding="utf-8") as f:
            return f.read().strip()
    except FileNotFoundError:
        return ""


def validar_chave(chave: str) -> str | None:
    """None se a Groq aceitar; senão, a mensagem para mostrar."""
    req = urllib.request.Request("https://api.groq.com/openai/v1/models",
                                 headers={"Authorization": f"Bearer {chave}", "User-Agent": "renantalk/1.0"})
    try:
        with urllib.request.urlopen(req, timeout=15) as r:
            json.load(r)
        return None
    except urllib.error.HTTPError as e:
        return "A Groq recusou essa chave." if e.code in (401, 403) else f"A Groq respondeu erro {e.code}."
    except Exception:  # noqa: BLE001
        return "Sem conexão com a Groq. Tente de novo."


def janela_chave() -> None:
    import tkinter as tk
    from tkinter import ttk

    root = tk.Tk()
    root.title(NOME)
    root.resizable(False, False)
    root.attributes("-topmost", True)
    try:
        root.iconbitmap(ICONE)
    except tk.TclError:
        pass
    f = ttk.Frame(root, padding=18)
    f.pack()
    ttk.Label(f, text="Chave da API Groq", font=("Segoe UI Semibold", 11)).grid(row=0, column=0, columnspan=2, sticky="w")
    atual = ler_chave()
    dica = f"Atual: {atual[:4]}…{atual[-4:]}" if len(atual) > 8 else "Nenhuma chave salva"
    ttk.Label(f, text=dica, foreground="#64748b").grid(row=1, column=0, columnspan=2, sticky="w", pady=(2, 10))
    var = tk.StringVar()
    ent = ttk.Entry(f, textvariable=var, show="•", width=46)
    ent.grid(row=2, column=0, columnspan=2, sticky="we")
    status = ttk.Label(f, text="", foreground="#64748b")
    status.grid(row=3, column=0, columnspan=2, sticky="w", pady=(6, 10))
    link = ttk.Label(f, text="Criar chave em console.groq.com", foreground="#2563eb", cursor="hand2")
    link.grid(row=4, column=0, sticky="w")
    link.bind("<Button-1>", lambda _e: os.startfile("https://console.groq.com/keys"))

    def salvar(_e=None) -> None:
        k = var.get().strip()
        if not k.startswith("gsk_"):
            status.config(text="A chave da Groq começa com gsk_.", foreground="#dc2626")
            return
        status.config(text="Validando na Groq…", foreground="#64748b")
        b_salvar.state(["disabled"])
        root.update()
        erro = validar_chave(k)
        b_salvar.state(["!disabled"])
        if erro:
            status.config(text=erro, foreground="#dc2626")
            return
        with open(ARQ_CHAVE, "w", encoding="utf-8") as fh:
            fh.write(k)
        status.config(text="Chave salva. Já está em uso.", foreground="#16a34a")
        root.after(900, root.destroy)

    botoes = ttk.Frame(f)
    botoes.grid(row=4, column=1, sticky="e")
    ttk.Button(botoes, text="Cancelar", command=root.destroy).pack(side="left", padx=(0, 6))
    b_salvar = ttk.Button(botoes, text="Salvar", command=salvar, default="active")
    b_salvar.pack(side="left")
    root.bind("<Return>", salvar)
    root.bind("<Escape>", lambda _e: root.destroy())
    root.update_idletasks()
    w, h = root.winfo_width(), root.winfo_height()
    root.geometry(f"+{(root.winfo_screenwidth() - w) // 2}+{(root.winfo_screenheight() - h) // 2}")
    root.after(50, ent.focus_force)
    root.mainloop()


# ---------- bandeja ----------
class Bandeja:
    def __init__(self, ao_sair, ao_atualizar, versao: str) -> None:
        self.ao_sair = ao_sair
        self.ao_atualizar = ao_atualizar
        self.versao = versao
        self.janela_aberta = threading.Lock()
        gerar_icone()
        threading.Thread(target=self._loop, daemon=True).start()

    def _dados(self) -> NOTIFYICONDATAW:
        d = NOTIFYICONDATAW()
        d.cbSize = ctypes.sizeof(d)
        d.hWnd, d.uID = self.hwnd, 1
        d.uFlags = 0x1 | 0x2 | 0x4  # MESSAGE | ICON | TIP
        d.uCallbackMessage = WM_TRAY
        d.hIcon = self.hicon
        d.szTip = f"{NOME} v{self.versao.split(' ')[0]} · segure Ctrl+Win para ditar"
        return d

    def notificar(self, texto: str) -> None:
        d = self._dados()
        d.uFlags = 0x10  # NIF_INFO: balão do Windows
        d.szInfoTitle = NOME
        d.szInfo = texto
        d.dwInfoFlags = 0x4 | 0x20  # NIIF_USER (ícone do app) | NIIF_LARGE_ICON
        s32.Shell_NotifyIconW(1, ctypes.byref(d))  # NIM_MODIFY

    def _adicionar(self) -> None:
        s32.Shell_NotifyIconW(0, ctypes.byref(self._dados()))  # NIM_ADD

    def remover(self) -> None:
        s32.Shell_NotifyIconW(2, ctypes.byref(self._dados()))  # NIM_DELETE

    def _menu(self) -> None:
        m = u32.CreatePopupMenu()
        u32.AppendMenuW(m, 0x1, 0, f"{NOME} · Ctrl+Win para ditar")  # MF_GRAYED
        u32.AppendMenuW(m, 0x1, 0, f"v{self.versao}")
        u32.AppendMenuW(m, 0x800, 0, None)  # separador
        u32.AppendMenuW(m, 0, ITEM_CHAVE, "Trocar chave Groq…")
        u32.AppendMenuW(m, 0, ITEM_ATUALIZAR, "Procurar atualização")
        u32.AppendMenuW(m, 0, ITEM_LOG, "Ver log")
        u32.AppendMenuW(m, 0x800, 0, None)
        u32.AppendMenuW(m, 0, ITEM_SAIR, f"Sair do {NOME}")
        p = W.POINT()
        u32.GetCursorPos(ctypes.byref(p))
        u32.SetForegroundWindow(self.hwnd)  # sem isso o menu não fecha ao clicar fora
        item = u32.TrackPopupMenu(m, 0x100 | 0x2, p.x, p.y, 0, self.hwnd, None)  # RETURNCMD | RIGHTBUTTON
        u32.PostMessageW(self.hwnd, 0, 0, 0)
        u32.DestroyMenu(m)
        if item == ITEM_CHAVE:
            self.abrir_chave()
        elif item == ITEM_ATUALIZAR:
            threading.Thread(target=self.ao_atualizar, args=(True,), daemon=True).start()
        elif item == ITEM_LOG and os.path.exists(ARQ_LOG):
            os.startfile(ARQ_LOG)
        elif item == ITEM_SAIR:
            self.remover()
            self.ao_sair()

    def abrir_chave(self) -> None:
        def rodar() -> None:
            if not self.janela_aberta.acquire(blocking=False):
                return  # já está aberta
            try:
                janela_chave()
            finally:
                self.janela_aberta.release()
        threading.Thread(target=rodar, daemon=True).start()

    def _proc_janela(self, h, msg, wp, lp):
        if msg == WM_TRAY and lp in (0x202, 0x205):  # LBUTTONUP, RBUTTONUP
            self._menu()
            return 0
        if msg == self.msg_barra:  # Explorer reiniciou: recoloca o ícone
            self._adicionar()
            return 0
        return u32.DefWindowProcW(h, msg, wp, lp)

    def _loop(self) -> None:
        self._proc = WNDPROC(self._proc_janela)
        inst = k32.GetModuleHandleW(None)
        wc = WNDCLASSW(lpfnWndProc=self._proc, hInstance=inst, lpszClassName="RenanTalkBandeja")
        u32.RegisterClassW(ctypes.byref(wc))
        self.hwnd = u32.CreateWindowExW(0, "RenanTalkBandeja", NOME, 0, 0, 0, 0, 0, None, None, inst, None)
        self.msg_barra = u32.RegisterWindowMessageW("TaskbarCreated")
        tam = u32.GetSystemMetrics(49)  # SM_CXSMICON
        self.hicon = u32.LoadImageW(None, ICONE, 1, tam, tam, 0x10)  # IMAGE_ICON, LR_LOADFROMFILE
        self._adicionar()
        msg = W.MSG()
        while u32.GetMessageW(ctypes.byref(msg), None, 0, 0) > 0:
            u32.TranslateMessage(ctypes.byref(msg))
            u32.DispatchMessageW(ctypes.byref(msg))
