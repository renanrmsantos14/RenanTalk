"""Ícone do RenanTalk na área de notificação do Windows (perto do relógio), com menu."""
import ctypes
import os
import threading
from ctypes import wintypes as W

import ajustes
from comum import (ARQ_LOG, ICONE, ICONES, NOME, abrir_configuracoes, definir_inicio_automatico,
                   inicio_automatico)
from luz import WNDCLASSW, WNDPROC, k32, u32

WM_TRAY = 0x8000 + 1
ITEM_LOG, ITEM_ATUALIZAR, ITEM_SAIR, ITEM_VOCAB, ITEM_INICIO, ITEM_COLAR = 2, 3, 4, 5, 6, 7

s32 = ctypes.windll.shell32


class GUID(ctypes.Structure):
    _fields_ = [("a", W.DWORD), ("b", W.WORD), ("c", W.WORD), ("d", ctypes.c_ubyte * 8)]


class NOTIFYICONDATAW(ctypes.Structure):
    _fields_ = [("cbSize", W.DWORD), ("hWnd", W.HWND), ("uID", W.UINT), ("uFlags", W.UINT), ("uCallbackMessage", W.UINT),
                ("hIcon", W.HICON), ("szTip", W.WCHAR * 128), ("dwState", W.DWORD), ("dwStateMask", W.DWORD),
                ("szInfo", W.WCHAR * 256), ("uVersion", W.UINT), ("szInfoTitle", W.WCHAR * 64), ("dwInfoFlags", W.DWORD),
                ("guidItem", GUID), ("hBalloonIcon", W.HICON)]


class MENUITEMINFOW(ctypes.Structure):
    _fields_ = [("cbSize", W.UINT), ("fMask", W.UINT), ("fType", W.UINT), ("fState", W.UINT), ("wID", W.UINT),
                ("hSubMenu", W.HMENU), ("hbmpChecked", W.HBITMAP), ("hbmpUnchecked", W.HBITMAP),
                ("dwItemData", ctypes.c_size_t), ("dwTypeData", W.LPWSTR), ("cch", W.UINT), ("hbmpItem", W.HBITMAP)]


class BMIH(ctypes.Structure):
    _fields_ = [("biSize", W.DWORD), ("biWidth", W.LONG), ("biHeight", W.LONG), ("biPlanes", W.WORD),
                ("biBitCount", W.WORD), ("biCompression", W.DWORD), ("biSizeImage", W.DWORD),
                ("biXPelsPerMeter", W.LONG), ("biYPelsPerMeter", W.LONG), ("biClrUsed", W.DWORD), ("biClrImportant", W.DWORD)]


g32 = ctypes.windll.gdi32
g32.CreateDIBSection.argtypes = [W.HDC, ctypes.c_void_p, W.UINT, ctypes.POINTER(ctypes.c_void_p), W.HANDLE, W.DWORD]
g32.CreateDIBSection.restype = W.HBITMAP
u32.SetMenuItemInfoW.argtypes = [W.HMENU, W.UINT, W.BOOL, ctypes.POINTER(MENUITEMINFOW)]
u32.SetMenuDefaultItem.argtypes = [W.HMENU, W.UINT, W.UINT]
u32.GetMenuItemCount.argtypes = [W.HMENU]
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
    def __init__(self, ao_sair, ao_atualizar, versao: str, ao_colar=None) -> None:
        self.ao_colar = ao_colar
        self.ao_sair = ao_sair
        self.ao_atualizar = ao_atualizar
        self.versao = versao
        self._bitmaps: dict[str, int] = {}
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
        if not ajustes.ler()["notificacoes"]:  # desligadas: erro continua no bipe grave e no log
            return
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

    def _bitmap(self, nome: str):
        """Bitmap 32 bits com transparência (gerado de SVG em app/icones) no tamanho de ícone de menu do sistema."""
        if nome not in self._bitmaps:
            alvo = u32.GetSystemMetrics(71)  # SM_CXMENUCHECK: ~16 px a 100%, maior em telas com escala
            tam = min((16, 20, 24, 32), key=lambda t: abs(t - alvo))
            caminho = os.path.join(ICONES, f"{nome}-{tam}.bgra")
            if not os.path.exists(caminho):
                return None
            dados = open(caminho, "rb").read()
            bi = BMIH(biSize=ctypes.sizeof(BMIH), biWidth=tam, biHeight=-tam, biPlanes=1, biBitCount=32)
            bits = ctypes.c_void_p()
            hbmp = g32.CreateDIBSection(None, ctypes.byref(bi), 0, ctypes.byref(bits), None, 0)
            ctypes.memmove(bits, dados, len(dados))
            self._bitmaps[nome] = hbmp
        return self._bitmaps[nome]

    def _item(self, m, ident: int, texto: str, icone: str | None = None, flags: int = 0) -> None:
        u32.AppendMenuW(m, flags, ident, texto)
        if icone and (bmp := self._bitmap(icone)):
            info = MENUITEMINFOW(cbSize=ctypes.sizeof(MENUITEMINFOW), fMask=0x80, hbmpItem=bmp)  # MIIM_BITMAP
            u32.SetMenuItemInfoW(m, u32.GetMenuItemCount(m) - 1, True, ctypes.byref(info))

    def _menu(self) -> None:
        m = u32.CreatePopupMenu()
        self._item(m, 0, f"{NOME}   ·   v{self.versao.split(' ')[0]}", "marca")
        self._item(m, 0, "Segure Ctrl + Win para ditar", flags=0x1)  # MF_GRAYED
        u32.AppendMenuW(m, 0x800, 0, None)  # separador
        self._item(m, ITEM_VOCAB, "Configurações…", "menu-config")
        self._item(m, ITEM_COLAR, "Colar último ditado")
        self._item(m, ITEM_ATUALIZAR, "Procurar atualização", "menu-atualizar")
        self._item(m, ITEM_LOG, "Ver log", "menu-log")
        self._item(m, ITEM_INICIO, "Iniciar com o Windows", flags=0x8 if inicio_automatico() else 0)  # MF_CHECKED
        u32.AppendMenuW(m, 0x800, 0, None)
        self._item(m, ITEM_SAIR, f"Sair do {NOME}", "menu-sair")
        u32.SetMenuDefaultItem(m, ITEM_VOCAB, False)  # ação principal em negrito
        p = W.POINT()
        u32.GetCursorPos(ctypes.byref(p))
        u32.SetForegroundWindow(self.hwnd)  # sem isso o menu não fecha ao clicar fora
        item = u32.TrackPopupMenu(m, 0x100 | 0x2, p.x, p.y, 0, self.hwnd, None)  # RETURNCMD | RIGHTBUTTON
        u32.PostMessageW(self.hwnd, 0, 0, 0)
        u32.DestroyMenu(m)
        if item == ITEM_VOCAB:  # listar os microfones pode demorar: fora do laço de mensagens da bandeja
            threading.Thread(target=self.abrir_configuracoes, daemon=True).start()
        elif item == ITEM_COLAR and self.ao_colar:
            self.ao_colar()  # o foco ainda volta do menu para a janela anterior: o ditado.pyw espera um instante
        elif item == ITEM_ATUALIZAR:
            threading.Thread(target=self.ao_atualizar, args=(True,), daemon=True).start()
        elif item == ITEM_LOG and os.path.exists(ARQ_LOG):
            os.startfile(ARQ_LOG)
        elif item == ITEM_INICIO:
            threading.Thread(target=definir_inicio_automatico, args=(not inicio_automatico(),), daemon=True).start()
        elif item == ITEM_SAIR:
            self.remover()
            self.ao_sair()

    def abrir_configuracoes(self, aba: str = "voc") -> None:
        abrir_configuracoes(aba)  # processo à parte: só consome memória enquanto a janela está aberta

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
        self.msg_barra = u32.RegisterWindowMessageW("TaskbarCreated")  # antes: a criação já dispara mensagens
        self.hwnd = u32.CreateWindowExW(0, "RenanTalkBandeja", NOME, 0, 0, 0, 0, 0, None, None, inst, None)
        tam = u32.GetSystemMetrics(49)  # SM_CXSMICON
        self.hicon = u32.LoadImageW(None, ICONE, 1, tam, tam, 0x10)  # IMAGE_ICON, LR_LOADFROMFILE
        self._adicionar()
        msg = W.MSG()
        while u32.GetMessageW(ctypes.byref(msg), None, 0, 0) > 0:
            u32.TranslateMessage(ctypes.byref(msg))
            u32.DispatchMessageW(ctypes.byref(msg))
