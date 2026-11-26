"""Atualização automática pelos releases do GitHub (repositório público, sem token).

Lê o último release, compara com app/VERSION, baixa RenanTalk.zip, copia por cima
da pasta do app e instala dependências novas. Quem reinicia o programa é o ditado.
"""
import io
import json
import os
import shutil
import subprocess
import sys
import tempfile
import urllib.request
import zipfile

REPO = "renanrmsantos14/RenanTalk"
APP = os.path.dirname(os.path.abspath(__file__))
SEM_JANELA = 0x08000000  # CREATE_NO_WINDOW
def versao_texto() -> str:
    try:
        with open(os.path.join(APP, "VERSION"), encoding="utf-8") as f:
            return f.read().strip()
    except FileNotFoundError:
        return "0.0.0 · dev"


CAB = {"User-Agent": f"renantalk/{versao_texto().split(' ')[0]}", "Accept": "application/vnd.github+json"}


def eh_dev() -> bool:
    return "dev" in versao_texto()


def _tupla(v: str) -> tuple[int, ...]:
    return tuple(int(p) for p in v.lstrip("v").split("·")[0].strip().split("."))


def _baixar(url: str) -> bytes:
    with urllib.request.urlopen(urllib.request.Request(url, headers=CAB), timeout=120) as r:
        return r.read()


def _ler(caminho: str) -> str:
    try:
        with open(caminho, encoding="utf-8") as f:
            return f.read()
    except FileNotFoundError:
        return ""


def verificar_e_aplicar() -> tuple[str, str]:
    """Retorna (situação, detalhe). Situação: 'atualizado', 'em_dia', 'dev' ou 'erro'."""
    if eh_dev():
        return "dev", "versão de desenvolvimento: atualização automática desligada"
    try:
        rel = json.loads(_baixar(f"https://api.github.com/repos/{REPO}/releases/latest"))
        nova = rel["tag_name"].lstrip("v")
        if _tupla(nova) <= _tupla(versao_texto()):
            return "em_dia", f"já está na versão mais recente (v{nova})"
        asset = next((a for a in rel["assets"] if a["name"] == "RenanTalk.zip"), None)
        if not asset:
            return "erro", "release sem o arquivo RenanTalk.zip"
        pacote = _baixar(asset["browser_download_url"])
        req_antes = _ler(os.path.join(APP, "requirements.txt"))
        with tempfile.TemporaryDirectory() as tmp:
            zipfile.ZipFile(io.BytesIO(pacote)).extractall(tmp)
            if not os.path.exists(os.path.join(tmp, "ditado.pyw")):
                return "erro", "pacote do release sem ditado.pyw"
            shutil.copytree(tmp, APP, dirs_exist_ok=True)  # .py não fica travado no Windows: dá para sobrescrever
        if _ler(os.path.join(APP, "requirements.txt")) != req_antes:
            req = os.path.join(APP, "requirements.txt")
            if sys.platform == "win32":
                python = os.path.join(os.path.dirname(sys.executable), "python.exe")  # pythonw não roda pip
                subprocess.run([python, "-m", "pip", "install", "--user", "-q", "-r", req],
                               timeout=600, creationflags=SEM_JANELA)
            else:
                subprocess.run([sys.executable, "-m", "pip", "install", "--user", "--break-system-packages", "-q",
                                "-r", req], timeout=600)
        return "atualizado", f"v{nova}"
    except Exception as e:  # noqa: BLE001 - sem internet, GitHub fora etc.: tenta de novo mais tarde
        return "erro", repr(e)
