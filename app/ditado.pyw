"""RenanTalk: ditado por voz com Groq (Whisper large-v3), para Windows e Linux (X11).

Segure Ctrl + Win (Ctrl + Super no Linux), fale e solte: o texto é colado onde estiver o cursor.
Ícone na bandeja: configurações, procurar atualização, ver log, sair.
Dados (chave, vocabulário, log): %APPDATA%\\RenanTalk no Windows, ~/.config/RenanTalk no Linux.
"""
import os

for _v in ("OPENBLAS_NUM_THREADS", "OMP_NUM_THREADS", "MKL_NUM_THREADS"):
    os.environ[_v] = "1"  # numpy com 1 thread: sem isso reserva ~350 MB para 10 núcleos

import io
import json
import sys
import threading
import time
import urllib.request
import uuid
import wave

import numpy as np
import sounddevice as sd

import ajustes
import atualizador
import historico
from comum import ARQ_LOG, WINDOWS, ler_chave, prompt_vocabulario

if WINDOWS:
    import plataforma_windows as plat
else:
    import plataforma_linux as plat

LUZ = plat.Luz()
INTERVALO_ATUALIZACAO = 6 * 3600  # s
URL = "https://api.groq.com/openai/v1/audio/transcriptions"
MODELO = "whisper-large-v3"
TAXA = 16000
MINIMO_S = 0.4  # gravações mais curtas são ignoradas (toque acidental)
SILENCIO = 800  # pico abaixo disso (de 32767) é tratado como silêncio

bandeja = None
gravando = False
quadros: list[bytes] = []
inicio = 0.0
outra_tecla = False
trava = threading.Lock()


def log(msg: str) -> None:
    with open(ARQ_LOG, "a", encoding="utf-8") as f:
        f.write(time.strftime("%d/%m/%Y %H:%M:%S ") + msg + "\n")


def ao_audio(dados, _frames, _tempo, _status) -> None:
    if gravando:
        quadros.append(bytes(dados))
        a = np.frombuffer(dados, np.int16).astype(np.float32)
        LUZ.audio(min(1.0, float(np.sqrt(np.mean(a * a))) / 2500))


fluxo = None
fluxo_mic = None  # microfone com que o fluxo foi aberto ("" = padrão do sistema)


def garantir_fluxo() -> None:
    """Abre o fluxo no microfone dos ajustes; trocar o microfone recria o fluxo no próximo ditado."""
    global fluxo, fluxo_mic
    mic = ajustes.ler()["microfone"]
    if fluxo is not None and mic == fluxo_mic:
        return
    if fluxo is not None:
        fluxo.close()
        fluxo = None
    dispositivo = ajustes.indice_do_microfone(mic, list(sd.query_devices())) if mic else None
    try:
        fluxo = sd.RawInputStream(samplerate=TAXA, channels=1, dtype="int16", callback=ao_audio, device=dispositivo)
    except Exception as e:  # noqa: BLE001 - dispositivo ocupado ou inválido: cai no padrão
        log(f"microfone {mic!r} não abriu ({e!r}), usando o padrão do sistema")
        fluxo = sd.RawInputStream(samplerate=TAXA, channels=1, dtype="int16", callback=ao_audio)
    fluxo_mic = mic


def comecar() -> None:
    global gravando, inicio, outra_tecla
    with trava:
        if gravando:
            return
        quadros.clear()
        outra_tecla = False
        inicio = time.time()
        garantir_fluxo()
        fluxo.start()
        gravando = True
    LUZ.definir("rec")
    plat.bipe(880, 60)


def marcar_outra_tecla(qual: str) -> None:
    global outra_tecla
    if not outra_tecla:
        log(f"outra tecla: {qual}")
    outra_tecla = True  # Ctrl+Win+outra tecla = atalho do sistema, não ditado


def parar() -> None:
    global gravando
    with trava:
        if not gravando:
            return
        gravando = False
        fluxo.stop()
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
    plat.bipe(660, 60)
    threading.Thread(target=transcrever_e_colar, args=(audio, duracao), daemon=True).start()


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
    # Vocabulário vai como contexto (não como ordem) para o Whisper grafar certo; relido a cada ditado.
    campos = {"model": MODELO, "temperature": "0", "response_format": "json"}
    if (idioma := ajustes.ler()["idioma"]) != "auto":  # automático: sem `language`, o Whisper detecta
        campos["language"] = idioma
    if vocab := prompt_vocabulario():
        campos["prompt"] = vocab
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
            "Authorization": f"Bearer {ler_chave()}",  # relida a cada uso: troca nas configurações vale na hora
            "Content-Type": f"multipart/form-data; boundary={limite}",
            "User-Agent": "groq-ditado/1.0",  # o padrão Python-urllib leva 403
        },
    )
    with urllib.request.urlopen(req, timeout=30) as r:
        return json.load(r)["text"].strip()


def guardar_no_historico(texto: str, duracao: float) -> None:
    """Guarda o ditado no histórico local. Qualquer falha é ignorada: o texto precisa ser colado."""
    try:
        historico.gravar(texto, app=plat.app_ativo(), idioma=ajustes.ler()["idioma"], duracao_s=duracao)
    except Exception as e:  # noqa: BLE001
        log(f"histórico: falhou ({type(e).__name__})")


def transcrever_e_colar(audio: bytes, duracao: float = 0.0) -> None:
    try:
        texto = transcrever(audio)
        log(f"transcrito {len(texto)} caracteres")
        if texto:
            guardar_no_historico(texto, duracao)
            plat.colar(texto + " ")
        LUZ.definir("done")
    except Exception as e:  # noqa: BLE001 - qualquer falha vira log, luz vermelha e bipe grave
        log(f"erro: {e!r}")
        LUZ.definir("erro")
        plat.bipe(300, 250)


def reiniciar() -> None:
    """Fecha esta instância e abre a versão nova que acabou de ser copiada."""
    import subprocess

    bandeja.remover()
    plat.liberar_instancia()
    subprocess.Popen([sys.executable, os.path.abspath(__file__)], cwd=os.path.dirname(os.path.abspath(__file__)),
                     stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                     start_new_session=sys.platform != "win32")  # não herda o terminal de quem chamou
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
    time.sleep(60)  # deixa o sistema terminar de iniciar
    while True:
        atualizar()
        time.sleep(INTERVALO_ATUALIZACAO)


if __name__ == "__main__":
    if not plat.instancia_unica():
        raise SystemExit  # já está rodando
    atualizador.limpar_antigos()
    versao = atualizador.versao_texto()
    log(f"iniciado v{versao} ({sys.platform})")
    bandeja = plat.Bandeja(ao_sair=lambda: os._exit(0), ao_atualizar=atualizar, versao=versao)
    if not ler_chave():
        bandeja.abrir_configuracoes("key")
    threading.Thread(target=ciclo_atualizacao, daemon=True).start()
    threading.Thread(target=ajustes.escrever_dispositivos, daemon=True).start()
    threading.Thread(target=historico.ciclo_retencao, daemon=True).start()  # apaga > 30 dias ao iniciar e 1x por dia
    plat.rodar(comecar, parar, marcar_outra_tecla, lambda: gravando)
