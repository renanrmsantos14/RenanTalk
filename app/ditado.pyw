"""RenanTalk: ditado por voz com Groq (Whisper large-v3).

Segure Ctrl + Win, fale e solte: o texto é colado onde estiver o cursor.
Ícone perto do relógio: trocar chave Groq, procurar atualização, ver log, sair.
Chave da Groq: %APPDATA%\\RenanTalk\\chave.txt (só a chave, uma linha).
Log: %APPDATA%\\RenanTalk\\ditado.log
"""
import os

for _v in ("OPENBLAS_NUM_THREADS", "OMP_NUM_THREADS", "MKL_NUM_THREADS"):
    os.environ[_v] = "1"  # numpy com 1 thread: sem isso reserva ~350 MB para 10 núcleos

import ctypes
import io
import json
import sys
import threading
import time
import urllib.request
import uuid
import wave
import winsound

import keyboard
import numpy as np
import pyperclip
import sounddevice as sd

import atualizador
from bandeja import ARQ_LOG, Bandeja, ler_chave
from luz import Luz

LUZ = Luz()
LOG = ARQ_LOG
INTERVALO_ATUALIZACAO = 6 * 3600  # s
bandeja = None
_mutex = None
URL = "https://api.groq.com/openai/v1/audio/transcriptions"
MODELO = "whisper-large-v3"
# Termos que o Whisper deve grafar certo (vai como contexto, não como ordem).
VOCABULARIO = (
    "Betinhos, Betinhos Executive Service, Dataverse, Power Apps, Power Automate, "
    "Power BI, SharePoint, Microsoft 365, RD Station, WebResource, Planner, Teams, WhatsApp."
)
TAXA = 16000
MINIMO_S = 0.4  # gravações mais curtas são ignoradas (toque acidental)
SILENCIO = 800  # pico abaixo disso (de 32767) é tratado como silêncio

gravando = False
quadros: list[bytes] = []
inicio = 0.0
outra_tecla = False
trava = threading.Lock()


def log(msg: str) -> None:
    with open(LOG, "a", encoding="utf-8") as f:
        f.write(time.strftime("%d/%m/%Y %H:%M:%S ") + msg + "\n")


def mascarar_win() -> None:
    # Toque numa tecla sem função (VK 0xE8) para o Windows não abrir o menu Iniciar ao soltar o Win.
    ctypes.windll.user32.keybd_event(0xE8, 0, 0, 0)
    ctypes.windll.user32.keybd_event(0xE8, 0, 2, 0)


def ao_audio(dados, _frames, _tempo, _status) -> None:
    if gravando:
        quadros.append(bytes(dados))
        a = np.frombuffer(dados, np.int16).astype(np.float32)
        LUZ.audio(min(1.0, float(np.sqrt(np.mean(a * a))) / 2500))


fluxo = sd.RawInputStream(samplerate=TAXA, channels=1, dtype="int16", callback=ao_audio)


def comecar() -> None:
    global gravando, inicio, outra_tecla
    quadros.clear()
    outra_tecla = False
    inicio = time.time()
    fluxo.start()
    gravando = True
    LUZ.definir("rec")
    winsound.Beep(880, 60)


def parar() -> None:
    global gravando
    gravando = False
    fluxo.stop()
    mascarar_win()
    duracao = time.time() - inicio
    log(f"gravou {duracao:.1f}s, outra_tecla={outra_tecla}")
    if outra_tecla or duracao < MINIMO_S:
        LUZ.definir("done")
        return
    audio = b"".join(quadros)
    if not audio or np.abs(np.frombuffer(audio, np.int16).astype(np.int32)).max() < SILENCIO:
        log("ignorado: sem voz")  # o Whisper inventa texto quando recebe silêncio
        LUZ.definir("done")
        return
    LUZ.definir("proc")
    winsound.Beep(660, 60)
    threading.Thread(target=transcrever_e_colar, args=(audio,), daemon=True).start()


def wav(audio: bytes) -> bytes:
    buf = io.BytesIO()
    with wave.open(buf, "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(TAXA)
        w.writeframes(audio)
    return buf.getvalue()


def transcrever(audio: bytes) -> str:
    limite = uuid.uuid4().hex
    campos = {"model": MODELO, "language": "pt", "temperature": "0", "prompt": VOCABULARIO, "response_format": "json"}
    corpo = io.BytesIO()
    for nome, valor in campos.items():
        corpo.write(f'--{limite}\r\nContent-Disposition: form-data; name="{nome}"\r\n\r\n{valor}\r\n'.encode())
    corpo.write(
        f'--{limite}\r\nContent-Disposition: form-data; name="file"; filename="audio.wav"\r\n'
        "Content-Type: audio/wav\r\n\r\n".encode()
    )
    corpo.write(wav(audio))
    corpo.write(f"\r\n--{limite}--\r\n".encode())
    req = urllib.request.Request(
        URL,
        data=corpo.getvalue(),
        headers={
            "Authorization": f"Bearer {ler_chave()}",  # relida a cada uso: troca pelo menu vale na hora
            "Content-Type": f"multipart/form-data; boundary={limite}",
            "User-Agent": "groq-ditado/1.0",  # o padrão Python-urllib leva 403
        },
    )
    with urllib.request.urlopen(req, timeout=30) as r:
        return json.load(r)["text"].strip()


def colar(texto: str) -> None:
    anterior = pyperclip.paste()
    pyperclip.copy(texto)
    time.sleep(0.05)
    keyboard.send("ctrl+v")
    time.sleep(0.3)
    pyperclip.copy(anterior)  # devolve o que estava na área de transferência


def transcrever_e_colar(audio: bytes) -> None:
    try:
        texto = transcrever(audio)
        log(f"transcrito {len(texto)} caracteres")
        if texto:
            colar(texto + " ")
        LUZ.definir("done")
    except Exception as e:  # noqa: BLE001 - qualquer falha vira log, luz vermelha e bipe grave
        log(f"erro: {e!r}")
        LUZ.definir("erro")
        winsound.Beep(300, 250)


def ao_teclado(ev: keyboard.KeyboardEvent) -> None:
    global outra_tecla
    nome = (ev.name or "").lower()
    eh_ctrl = "ctrl" in nome
    eh_win = nome in ("windows", "left windows", "right windows")
    with trava:
        if ev.event_type == "down":
            if gravando and not (eh_ctrl or eh_win):
                outra_tecla = True  # Ctrl+Win+outra tecla = atalho do Windows, não ditado
                log(f"outra tecla: {ev.name!r} scan={ev.scan_code}")
            elif not gravando and (eh_ctrl or eh_win) and keyboard.is_pressed("ctrl") and (
                keyboard.is_pressed("windows") or keyboard.is_pressed("left windows") or eh_win
            ):
                comecar()
        elif ev.event_type == "up" and gravando and (eh_ctrl or eh_win):
            parar()


def reiniciar() -> None:
    """Fecha esta instância e abre a versão nova que acabou de ser copiada."""
    import subprocess

    bandeja.remover()
    ctypes.windll.kernel32.CloseHandle(_mutex)  # libera o "já está rodando" para a nova instância
    subprocess.Popen([sys.executable, os.path.abspath(__file__)], cwd=os.path.dirname(os.path.abspath(__file__)))
    os._exit(0)


def atualizar(manual: bool = False) -> None:
    situacao, detalhe = atualizador.verificar_e_aplicar()
    log(f"atualização: {situacao} {detalhe}")
    if situacao == "atualizado":
        while gravando:  # não interrompe um ditado em andamento
            time.sleep(1)
        bandeja.notificar(f"Atualizado para {detalhe}.")
        time.sleep(2)
        reiniciar()
    elif manual:
        bandeja.notificar({"em_dia": "Você já está na versão mais recente.",
                           "dev": "Versão de desenvolvimento: sem atualização automática."}.get(situacao,
                           "Não consegui procurar atualização agora. Veja o log."))


def ciclo_atualizacao() -> None:
    time.sleep(60)  # deixa o Windows terminar de iniciar
    while True:
        atualizar()
        time.sleep(INTERVALO_ATUALIZACAO)


if __name__ == "__main__":
    _k =ctypes.WinDLL("kernel32", use_last_error=True)
    _mutex = _k.CreateMutexW(None, False, "RenanTalk")
    if ctypes.get_last_error() == 183:  # ERROR_ALREADY_EXISTS: já está rodando
        raise SystemExit
    versao = atualizador.versao_texto()
    log(f"iniciado v{versao}")
    bandeja = Bandeja(ao_sair=lambda: os._exit(0), ao_atualizar=atualizar, versao=versao)
    if not ler_chave():
        bandeja.abrir_chave()
    threading.Thread(target=ciclo_atualizacao, daemon=True).start()
    keyboard.hook(ao_teclado)
    keyboard.wait()
