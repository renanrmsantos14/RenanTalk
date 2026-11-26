"""Ícone do RenanTalk na área de notificação do Windows (perto do relógio), com menu."""
import ctypes
import os
import threading
from ctypes import wintypes as W

from comum import ARQ_LOG, ICONE, NOME, gerar_icone
from luz import WNDCLASSW, WNDPROC, k32, u32

WM_TRAY = 0x8000 + 1
ITEM_LOG, ITEM_ATUALIZAR, ITEM_SAIR, ITEM_VOCAB = 2, 3, 4, 5

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


# ---------- bandeja ----------
class Bandeja:
    def __init__(self, ao_sair, ao_atualizar, versao: str) -> None:
        self.ao_sair = ao_sair
        self.ao_atualizar = ao_atualizar
        self.versao = versao
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
        u32.AppendMenuW(m, 0, ITEM_VOCAB, "Configurações…")
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
        if item == ITEM_VOCAB:
            self.abrir_configuracoes()
        elif item == ITEM_ATUALIZAR:
            threading.Thread(target=self.ao_atualizar, args=(True,), daemon=True).start()
        elif item == ITEM_LOG and os.path.exists(ARQ_LOG):
            os.startfile(ARQ_LOG)
        elif item == ITEM_SAIR:
            self.remover()
            self.ao_sair()

    def abrir_configuracoes(self, aba: str = "voc") -> None:
        """Processo à parte: só consome memória enquanto a janela está aberta."""
        import subprocess
        import sys

        app = os.path.dirname(os.path.abspath(__file__))
        subprocess.Popen([sys.executable, os.path.join(app, "configuracoes.py"), str(os.getpid()), aba], cwd=app)

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
