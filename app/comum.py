"""Partes do RenanTalk que valem para Windows e Linux: pastas, chave, vocabulário, ícone e validação."""
import json
import os
import struct
import sys
import urllib.error
import urllib.request
import zlib

import numpy as np

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
ICONE = os.path.join(PASTA, "renantalk.ico")  # Windows
ICONE_PNG = os.path.join(PASTA, "renantalk.png")  # Linux
VOCAB_PADRAO = ["Betinhos", "Betinhos Executive Service", "Dataverse", "Power Apps", "Power Automate", "Power BI",
                "SharePoint", "Microsoft 365", "RD Station", "WebResource", "Planner", "Teams", "WhatsApp"]
LIMITE_VOCAB = 850  # caracteres; o Whisper só aproveita ~224 tokens de contexto

os.makedirs(PASTA, exist_ok=True)
if ANTIGA and not os.path.exists(ARQ_CHAVE) and os.path.exists(os.path.join(ANTIGA, "chave.txt")):
    import shutil

    shutil.copy2(os.path.join(ANTIGA, "chave.txt"), ARQ_CHAVE)


def abrir(alvo: str) -> None:
    """Abre arquivo ou URL no programa padrão do sistema."""
    if WINDOWS:
        os.startfile(alvo)
    else:
        import subprocess

        subprocess.Popen(["xdg-open", alvo], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


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
    if not os.path.exists(ICONE_PNG):
        with open(ICONE_PNG, "wb") as f:
            f.write(_png_rgba(_desenho(64)))
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


# ---------- chave e vocabulário ----------
def ler_chave() -> str:
    try:
        with open(ARQ_CHAVE, encoding="utf-8") as f:
            return f.read().strip()
    except FileNotFoundError:
        return ""


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
