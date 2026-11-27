"""Partes do RenanTalk que valem para Windows e Linux: pastas, chave, vocabulário, ícone e validação."""
import json
import os
import sys
import time
import urllib.error
import urllib.request

NOME = "RenanTalk"
WINDOWS = sys.platform == "win32"
if WINDOWS:
    PASTA = os.path.join(os.environ["APPDATA"], "RenanTalk")
    ANTIGA = os.path.join(os.environ["APPDATA"], "GroqDitado")  # pasta da primeira versão
else:
    PASTA = os.path.join(os.environ.get("XDG_CONFIG_HOME") or os.path.expanduser("~/.config"), "RenanTalk")
    ANTIGA = ""
ARQ_CHAVE = os.path.join(PASTA, "chave.txt")
ARQ_LOG = os.path.join(PASTA, "ditado.log")
ARQ_VOCAB = os.path.join(PASTA, "vocabulario.txt")
APP_DIR = os.path.dirname(os.path.abspath(__file__))
ICONES = os.path.join(APP_DIR, "icones")  # gerados de assets/icone.svg
ICONE = os.path.join(ICONES, "renantalk.ico")  # Windows
ICONE_PNG = os.path.join(ICONES, "renantalk-64.png")  # Linux
VOCAB_PADRAO = ["Betinhos", "Betinhos Executive Service", "Dataverse", "Power Apps", "Power Automate", "Power BI",
                "SharePoint", "Microsoft 365", "RD Station", "WebResource", "Planner", "Teams", "WhatsApp"]
LIMITE_VOCAB = 850  # caracteres; o Whisper só aproveita ~224 tokens de contexto

os.makedirs(PASTA, exist_ok=True)
if ANTIGA and not os.path.exists(ARQ_CHAVE) and os.path.exists(os.path.join(ANTIGA, "chave.txt")):
    import shutil

    shutil.copy2(os.path.join(ANTIGA, "chave.txt"), ARQ_CHAVE)


def log(msg: str) -> None:
    """Uma linha no ditado.log (nunca o texto ditado). Falha de disco é ignorada."""
    try:
        with open(ARQ_LOG, "a", encoding="utf-8") as f:
            f.write(time.strftime("%d/%m/%Y %H:%M:%S ") + msg + "\n")
    except OSError:
        pass


def abrir(alvo: str) -> None:
    """Abre arquivo ou URL no programa padrão do sistema."""
    if WINDOWS:
        os.startfile(alvo)
    else:
        import subprocess

        subprocess.Popen(["xdg-open", alvo], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


_janela = None  # processo da última janela aberta por este ditado


def abrir_configuracoes(aba: str = "inicio") -> None:
    """Abre a janela de configurações: o executável Rust/Tauri se existir; senão, a versão Python + navegador."""
    import subprocess

    global _janela

    app = os.path.dirname(os.path.abspath(__file__))
    import ajustes

    ajustes.escrever_dispositivos()  # a janela (Rust) lê a lista de microfones deste arquivo
    exe = os.path.join(app, "renantalk-config.exe" if WINDOWS else "renantalk-config")
    if os.path.exists(exe):
        # python.exe (com console oculto pelo Rust) para o "Procurar atualização"; pythonw não devolve saída
        python = os.path.join(os.path.dirname(sys.executable), "python.exe") if WINDOWS else sys.executable
        _janela = subprocess.Popen([exe, str(os.getpid()), aba, python, app], cwd=app)
    else:
        aba_antiga = "voc" if aba == "inicio" else aba  # a janela antiga não tem Início
        _janela = subprocess.Popen([sys.executable, os.path.join(app, "configuracoes.py"), str(os.getpid()), aba_antiga], cwd=app)


def fechar_janela() -> bool:
    """Fecha a janela aberta por este ditado (numa atualização ela ficaria na versão antiga). True se estava aberta."""
    if _janela is None or _janela.poll() is not None:
        return False
    _janela.terminate()
    return True


def gerar_icone() -> None:
    """Mantido por compatibilidade: os ícones agora vêm prontos em app/icones."""


# ---------- início automático ----------
_ATALHO_WIN = os.path.join(os.environ.get("APPDATA", ""), "Microsoft", "Windows", "Start Menu", "Programs", "Startup",
                           "RenanTalk.lnk")
_AUTOSTART_LINUX = os.path.join(os.path.expanduser("~/.config/autostart"), "renantalk.desktop")


def inicio_automatico() -> bool:
    return os.path.exists(_ATALHO_WIN if WINDOWS else _AUTOSTART_LINUX)


def definir_inicio_automatico(ligar: bool) -> None:
    """Liga/desliga o início com o sistema (atalho na pasta Inicializar ou .desktop no autostart)."""
    import subprocess

    script = os.path.join(APP_DIR, "ditado.pyw")
    if WINDOWS:
        if not ligar:
            if os.path.exists(_ATALHO_WIN):
                os.remove(_ATALHO_WIN)
            return
        pythonw = os.path.join(os.path.dirname(sys.executable), "pythonw.exe")
        ps = ("$l=(New-Object -ComObject WScript.Shell).CreateShortcut($env:A); $l.TargetPath=$env:P; "
              "$l.Arguments='\"'+$env:S+'\"'; $l.WorkingDirectory=$env:D; $l.IconLocation=$env:I; "
              "$l.Description='RenanTalk - ditado por voz (Ctrl+Win)'; $l.Save()")
        env = dict(os.environ, A=_ATALHO_WIN, P=pythonw, S=script, D=APP_DIR, I=ICONE)
        subprocess.run(["powershell", "-NoProfile", "-Command", ps], env=env, capture_output=True,
                       creationflags=0x08000000)
        return
    if not ligar:
        if os.path.exists(_AUTOSTART_LINUX):
            os.remove(_AUTOSTART_LINUX)
        return
    os.makedirs(os.path.dirname(_AUTOSTART_LINUX), exist_ok=True)
    with open(_AUTOSTART_LINUX, "w", encoding="utf-8") as f:
        f.write("[Desktop Entry]\nType=Application\nName=RenanTalk\nComment=Ditado por voz (segure Ctrl+Super)\n"
                f"Exec=python3 {script}\nIcon={ICONE_PNG}\nX-GNOME-Autostart-enabled=true\n")


# ---------- chave e vocabulário ----------
def ler_chave() -> str:
    try:
        with open(ARQ_CHAVE, encoding="utf-8") as f:
            return f.read().strip()
    except FileNotFoundError:
        return ""


def cabecalhos_groq(tipo: str) -> dict:
    """Cabeçalhos das chamadas à Groq. A chave é relida a cada uso: a troca nas configurações vale na hora."""
    return {"Authorization": f"Bearer {ler_chave()}", "Content-Type": tipo,
            "User-Agent": "groq-ditado/1.0"}  # o padrão Python-urllib leva 403


def conferir_modelos(*modelos: str) -> None:
    """Avisa no log se a Groq desligou algum modelo que o app usa. Sem chave ou sem rede, fica quieto."""
    if not ler_chave():
        return
    try:
        req = urllib.request.Request("https://api.groq.com/openai/v1/models", headers=cabecalhos_groq("application/json"))
        with urllib.request.urlopen(req, timeout=10) as r:
            ativos = {m["id"] for m in json.load(r)["data"] if m.get("active", True)}
    except Exception:  # rede, JSON ou formato inesperado: a falha de verdade aparece no primeiro ditado
        return
    for m in modelos:
        if m not in ativos:
            log(f"modelo {m} não está ativo na Groq: atualize o app ou troque o modelo")


def salvar_chave(chave: str) -> None:
    with open(ARQ_CHAVE, "w", encoding="utf-8") as f:
        f.write(chave)
    if not WINDOWS:
        os.chmod(ARQ_CHAVE, 0o600)  # só o seu usuário lê


def ler_vocabulario() -> list[str]:
    """Uma palavra ou expressão por linha; linhas vazias e começadas com # são ignoradas."""
    if not os.path.exists(ARQ_VOCAB):
        with open(ARQ_VOCAB, "w", encoding="utf-8") as f:
            f.write("\n".join(VOCAB_PADRAO) + "\n")
    with open(ARQ_VOCAB, encoding="utf-8") as f:
        return [l.strip() for l in f if l.strip() and not l.lstrip().startswith("#")]


def prompt_vocabulario() -> str:
    texto = ""
    for termo in ler_vocabulario():  # corta no limite sem quebrar um termo ao meio
        if len(texto) + len(termo) + 2 > LIMITE_VOCAB:
            break
        texto += (", " if texto else "") + termo
    return texto + "." if texto else ""


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
