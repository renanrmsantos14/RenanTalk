"""Janela de configurações do RenanTalk (processo separado, só existe enquanto a janela está aberta).

Uso: python configuracoes.py <pid_do_ditado> [aba]
Sobe um servidor HTTP em 127.0.0.1 (porta aleatória, token na URL), abre ui.html numa janela de
aplicativo (Edge no Windows; Brave, Chromium ou Chrome no Linux, com Firefox como reserva) e encerra
sozinho quando a janela fecha (sem batimento por 8 s).
"""
import ctypes
import datetime
import json
import os
import secrets
import subprocess
import sys
import threading
import time
import shutil
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

import atualizador
from comum import (ARQ_LOG, ARQ_VOCAB, LIMITE_VOCAB, PASTA, WINDOWS, abrir, ler_chave, ler_vocabulario,
                   salvar_chave, validar_chave)

APP = os.path.dirname(os.path.abspath(__file__))
REPO_URL = f"https://github.com/{atualizador.REPO}"
TOKEN = secrets.token_urlsafe(16)
PID_DITADO = int(sys.argv[1]) if len(sys.argv) > 1 and sys.argv[1].isdigit() else 0
ABA = sys.argv[2] if len(sys.argv) > 2 else "voc"
ultimo_ping = time.time()


if WINDOWS:  # wintypes só existe de verdade no Windows
    from ctypes import wintypes as W

    class PMC(ctypes.Structure):
        _fields_ = [("cb", W.DWORD), ("PageFaultCount", W.DWORD)] + [(n, ctypes.c_size_t) for n in (
            "PeakWorkingSetSize", "WorkingSetSize", "QuotaPeakPagedPoolUsage", "QuotaPagedPoolUsage",
            "QuotaPeakNonPagedPoolUsage", "QuotaNonPagedPoolUsage", "PagefileUsage", "PeakPagefileUsage",
            "PrivateUsage")]


def memoria_mb(pid: int) -> int | None:
    if not WINDOWS:  # memória própria do processo (RssAnon), equivalente ao "privado" do Windows
        try:
            with open(f"/proc/{pid}/status") as f:
                for linha in f:
                    if linha.startswith("RssAnon:"):
                        return round(int(linha.split()[1]) / 1024)
        except OSError:
            return None
        return None
    k32 = ctypes.windll.kernel32
    k32.OpenProcess.restype = W.HANDLE
    h = k32.OpenProcess(0x1000 | 0x10, False, pid)  # QUERY_LIMITED_INFORMATION | VM_READ
    if not h:
        return None
    try:
        c = PMC(cb=ctypes.sizeof(PMC))
        if ctypes.windll.psapi.GetProcessMemoryInfo(W.HANDLE(h), ctypes.byref(c), c.cb):
            return round(c.PrivateUsage / 2**20)
    finally:
        k32.CloseHandle(W.HANDLE(h))
    return None


def estado() -> dict:
    hoje = datetime.date.today().strftime("%d/%m/%Y")
    ditados, verificacao = 0, ""
    if os.path.exists(ARQ_LOG):
        with open(ARQ_LOG, encoding="utf-8", errors="replace") as f:
            for linha in f:
                if linha.startswith(hoje) and "transcrito" in linha:
                    ditados += 1
                if "atualização:" in linha:
                    verificacao = linha[:16] + " · " + linha.split("atualização:", 1)[1].strip().split(" ")[0]
    chave = ler_chave()
    return {
        "versao": atualizador.versao_texto(),
        "vocab": ler_vocabulario(),
        "limite": LIMITE_VOCAB,
        "chave": f"{chave[:4]}{'•' * 10}{chave[-4:]}" if len(chave) > 8 else "",
        "memoria": memoria_mb(PID_DITADO) if PID_DITADO else None,
        "ditados_hoje": ditados,
        "verificacao": verificacao.replace("em_dia", "em dia").replace("atualizado", "atualizado").replace("erro", "falhou"),
        "aba": ABA,
    }


def reiniciar_ditado() -> None:
    if PID_DITADO:
        try:
            os.kill(PID_DITADO, 9)  # TerminateProcess; o mutex é liberado junto
        except OSError:
            pass
        time.sleep(0.5)
    subprocess.Popen([sys.executable, os.path.join(APP, "ditado.pyw")], cwd=APP)


class Api(BaseHTTPRequestHandler):
    def log_message(self, *a):  # silencioso
        pass

    def _ok(self, corpo, tipo="application/json; charset=utf-8", status=200) -> None:
        dados = corpo if isinstance(corpo, bytes) else json.dumps(corpo, ensure_ascii=False).encode()
        self.send_response(status)
        self.send_header("Content-Type", tipo)
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(dados)

    def _autorizado(self) -> bool:
        if f"/{TOKEN}/" not in self.path + "/":
            self._ok({"erro": "sem token"}, status=403)
            return False
        return True

    def do_GET(self) -> None:
        global ultimo_ping
        if not self._autorizado():
            return
        rota = self.path.split(TOKEN, 1)[1].split("?")[0]
        if rota in ("", "/"):
            with open(os.path.join(APP, "ui.html"), "rb") as f:
                self._ok(f.read(), "text/html; charset=utf-8")
        elif rota == "/api/estado":
            self._ok(estado())
        elif rota == "/api/ping":
            ultimo_ping = time.time()
            self._ok({"ok": True})
        else:
            self._ok({"erro": "rota"}, status=404)

    def do_POST(self) -> None:
        global ultimo_ping
        if not self._autorizado():
            return
        rota = self.path.split(TOKEN, 1)[1]
        n = int(self.headers.get("Content-Length") or 0)
        dados = json.loads(self.rfile.read(n) or b"{}")
        if rota == "/api/vocab":
            termos = [str(t).strip() for t in dados.get("termos", []) if str(t).strip()]
            with open(ARQ_VOCAB, "w", encoding="utf-8") as f:
                f.write("\n".join(termos) + "\n")
            self._ok({"ok": True})
        elif rota == "/api/chave":
            k = str(dados.get("chave", "")).strip()
            if not k.startswith("gsk_"):
                self._ok({"ok": False, "erro": "A chave da Groq começa com gsk_."})
                return
            erro = validar_chave(k)
            if erro:
                self._ok({"ok": False, "erro": erro})
                return
            salvar_chave(k)
            self._ok({"ok": True, "chave": f"{k[:4]}{'•' * 10}{k[-4:]}"})
        elif rota == "/api/atualizar":
            situacao, detalhe = atualizador.verificar_e_aplicar()
            with open(ARQ_LOG, "a", encoding="utf-8") as f:
                f.write(time.strftime("%d/%m/%Y %H:%M:%S ") + f"atualização: {situacao} {detalhe} (manual)\n")
            self._ok({"situacao": situacao, "detalhe": detalhe})
            if situacao == "atualizado":
                reiniciar_ditado()
        elif rota == "/api/abrir":
            alvo = {"log": ARQ_LOG, "repo": REPO_URL, "groq": "https://console.groq.com/keys"}.get(dados.get("alvo"))
            if alvo:
                abrir(alvo)
            self._ok({"ok": bool(alvo)})
        elif rota == "/api/fim":
            ultimo_ping = 0
            self._ok({"ok": True})
        else:
            self._ok({"erro": "rota"}, status=404)


def achar_edge() -> str | None:
    for base in (os.environ.get("ProgramFiles(x86)"), os.environ.get("ProgramFiles"), os.environ.get("LOCALAPPDATA")):
        if base:
            p = os.path.join(base, "Microsoft", "Edge", "Application", "msedge.exe")
            if os.path.exists(p):
                return p
    return None


def ajustar_janela(x: int, y: int, w: int, h: int) -> None:
    """O Edge reabre a janela de app como estava (às vezes maximizada): restaura, centraliza e traz para frente."""
    u32 = ctypes.windll.user32
    u32.FindWindowW.restype = W.HWND
    u32.SetWindowPos.argtypes = [W.HWND, W.HWND, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_int, W.UINT]
    u32.ShowWindow.argtypes = [W.HWND, ctypes.c_int]
    u32.SetForegroundWindow.argtypes = [W.HWND]
    for _ in range(100):  # até 10 s
        hwnd = u32.FindWindowW("Chrome_WidgetWin_1", "RenanTalk")
        if hwnd:
            u32.ShowWindow(hwnd, 9)  # SW_RESTORE
            u32.SetWindowPos(hwnd, None, x, y, w, h, 0x4)  # SWP_NOZORDER
            u32.SetForegroundWindow(hwnd)
            return
        time.sleep(0.1)


def ja_aberta() -> bool:
    if WINDOWS:
        k32 = ctypes.WinDLL("kernel32", use_last_error=True)
        k32.CreateMutexW(None, False, "RenanTalkConfiguracoes")
        return ctypes.get_last_error() == 183
    import fcntl

    global _trava
    _trava = open(os.path.join(PASTA, ".configuracoes"), "w")
    try:
        fcntl.flock(_trava, fcntl.LOCK_EX | fcntl.LOCK_NB)
        return False
    except OSError:
        return True


def abrir_linux(url: str) -> None:
    perfil = os.path.join(os.path.expanduser("~/.local/share/RenanTalk"), "ui-perfil")
    for nav in ("brave-browser", "chromium", "chromium-browser", "google-chrome"):
        if shutil.which(nav):
            subprocess.Popen([nav, f"--app={url}", f"--user-data-dir={perfil}", "--window-size=800,580",
                              "--no-first-run", "--no-default-browser-check", "--disable-extensions",
                              "--disable-sync", "--disable-background-networking", "--disable-component-update",
                              "--renderer-process-limit=1"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            return
    abrir(url)  # Firefox ou o navegador padrão: abre como janela comum


def main() -> None:
    if ja_aberta():
        return
    srv = ThreadingHTTPServer(("127.0.0.1", 0), Api)
    url = f"http://127.0.0.1:{srv.server_address[1]}/{TOKEN}/"
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    global ultimo_ping
    if not WINDOWS:
        abrir_linux(url)
        ultimo_ping = time.time() + 20
        while time.time() - ultimo_ping < 8:
            time.sleep(1)
        srv.shutdown()
        return
    u32 = ctypes.windll.user32
    try:
        u32.SetProcessDpiAwarenessContext(ctypes.c_void_p(-4))
    except Exception:  # noqa: BLE001
        pass
    escala = u32.GetDpiForSystem() / 96 if hasattr(u32, "GetDpiForSystem") else 1
    w, h = int(780 * escala), int(560 * escala)
    x, y = (u32.GetSystemMetrics(0) - w) // 2, (u32.GetSystemMetrics(1) - h) // 2
    edge = achar_edge()
    if edge:
        perfil = os.path.join(os.environ["LOCALAPPDATA"], "RenanTalk", "ui-perfil")
        subprocess.Popen([edge, f"--app={url}", f"--user-data-dir={perfil}", f"--window-size={w},{h}",
                          f"--window-position={x},{y}", "--no-first-run", "--no-default-browser-check",
                          "--disable-extensions", "--disable-sync", "--disable-background-networking",
                          "--disable-component-update", "--disable-default-apps", "--disable-breakpad",
                          "--disable-domain-reliability", "--disable-gpu", "--renderer-process-limit=1",
                          "--disable-features=Translate,OptimizationHints,MediaRouter,msEdgeSidebarV2,"
                          "msShoppingTrigger,EdgeCollections,msUndersideButton"])
        threading.Thread(target=ajustar_janela, args=(x, y, w, h), daemon=True).start()
    else:
        abrir(url)
    ultimo_ping = time.time() + 20  # tempo para o Edge abrir
    while time.time() - ultimo_ping < 8:
        time.sleep(1)
    srv.shutdown()


if __name__ == "__main__":
    main()
