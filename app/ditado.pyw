"""RenanTalk: ditado por voz com Groq (Whisper large-v3), para Windows e Linux (X11).

Segure Ctrl + Win (Ctrl + Super no Linux), fale e solte: o texto é colado onde estiver o cursor.
Ícone na bandeja: configurações, procurar atualização, ver log, sair.
Dados (chave, vocabulário, log): %APPDATA%\\RenanTalk no Windows, ~/.config/RenanTalk no Linux.
"""
import os

for _v in ("OPENBLAS_NUM_THREADS", "OMP_NUM_THREADS", "MKL_NUM_THREADS"):
    os.environ[_v] = "1"  # numpy com 1 thread: sem isso reserva ~350 MB para 10 núcleos

import concurrent.futures
import contextlib
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
import escrita
import historico
import substituicoes
from comum import WINDOWS, cabecalhos_groq, conferir_modelos, fechar_janela, ler_chave, log, prompt_vocabulario

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
traduzir = False  # o ditado em andamento veio do atalho de traduzir
trava = threading.Lock()
# Uma thread só para abaixar e restaurar o volume dos outros apps: mantém a ordem, não segura o hook do teclado
# e, no Windows, as sessões de áudio (COM) são usadas na mesma thread em que foram lidas.
fila_volume = concurrent.futures.ThreadPoolExecutor(max_workers=1)
abaixados: list = []


def abaixar_volume() -> None:
    global abaixados
    a = ajustes.ler()
    if a["abaixar_outros"]:
        abaixados = plat.abaixar_outros(a["volume_outros"] / 100)


def restaurar_volume() -> None:
    global abaixados
    feitos, abaixados = abaixados, []
    if feitos:
        plat.restaurar_outros(feitos)


def sair() -> None:
    with contextlib.suppress(Exception):  # sair no meio do ditado não deixa os outros apps baixos
        fila_volume.submit(restaurar_volume).result(timeout=3)
    os._exit(0)


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
        fluxo, fluxo_mic = abrir(dispositivo), mic
    except Exception as e:  # noqa: BLE001 - dispositivo ocupado ou inválido: cai no padrão
        log(f"microfone {mic!r} não abriu ({e!r}), usando o padrão do sistema")
        fluxo, fluxo_mic = abrir(), None  # None: tenta o escolhido de novo no próximo ditado


def abrir(dispositivo=None):
    return sd.RawInputStream(samplerate=TAXA, channels=1, dtype="int16", callback=ao_audio, device=dispositivo)


def iniciar_fluxo() -> None:
    """Abre e liga o fluxo. Microfone desligado depois de aberto: descarta o fluxo e tenta o padrão."""
    global fluxo, fluxo_mic
    garantir_fluxo()
    try:
        fluxo.start()
    except Exception as e:  # noqa: BLE001
        log(f"microfone parou de responder ({e!r}), reabrindo no padrão do sistema")
        with contextlib.suppress(Exception):
            fluxo.close()
        fluxo, fluxo_mic = abrir(), None
        fluxo.start()


def comecar(em_ingles: bool = False) -> None:
    global gravando, inicio, outra_tecla, traduzir
    with trava:
        if gravando:
            return
        quadros.clear()
        outra_tecla, traduzir = False, em_ingles
        inicio = time.time()
        try:
            iniciar_fluxo()
        except Exception as e:  # noqa: BLE001 - sem microfone nenhum: avisa em vez de falhar calado
            log(f"erro: nenhum microfone abriu ({e!r})")
            LUZ.definir("erro")
            plat.bipe(300, 250)
            return
        gravando = True
        fila_volume.submit(abaixar_volume)  # dentro da trava: o parar() não enfileira a restauração antes
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
        em_ingles = traduzir  # lido na trava: um comecar() logo depois não troca o modo deste ditado
        fluxo.stop()
        fila_volume.submit(restaurar_volume)
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
    threading.Thread(target=transcrever_e_colar, args=(audio, duracao, em_ingles), daemon=True).start()


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
        headers=cabecalhos_groq(f"multipart/form-data; boundary={limite}"),
    )
    with urllib.request.urlopen(req, timeout=30) as r:
        return json.load(r)["text"].strip()


def transcrever_e_colar(audio: bytes, duracao: float = 0.0, em_ingles: bool = False) -> None:
    try:
        texto = transcrever(audio)
        log(f"transcrito {len(texto)} caracteres")
        if texto:
            a = ajustes.ler()
            app = plat.app_ativo()  # com o foco na janela de destino: escolhe o modo; nunca levanta
            try:
                final = substituicoes.aplicar(texto, a["comandos"])
            except Exception as e:  # noqa: BLE001 - regra ou comando com defeito não pode perder o ditado: cola como veio
                log(f"substituições falharam ({type(e).__name__})")
                final = texto
            pedido = "ingles" if em_ingles else escrita.modo_para(app, a)
            final, modo = escrita.aplicar(final, pedido)  # nunca levanta
            plat.colar(final + " ")
            if modo != pedido and pedido in escrita.INSTRUCOES:  # a IA falhou: colou o literal, avisa com bipe grave
                plat.bipe(300, 150)
        LUZ.definir("done")  # antes de gravar: banco travado ou recálculo das sugestões não atrasam o texto nem a luz
        if texto:
            historico.gravar(final, app=app, modo=modo, idioma=a["idioma"], duracao_s=duracao, original=texto)  # nunca levanta
    except Exception as e:  # noqa: BLE001 - qualquer falha vira log, luz vermelha e bipe grave
        log(f"erro: {e!r}")
        LUZ.definir("erro")
        plat.bipe(300, 250)


def colar_ultimo(atraso: float = 0.0) -> None:
    """Cola o texto do ditado mais recente (já corrigido). Histórico vazio: bipe grave e nada colado.

    `atraso` deixa o menu da bandeja fechar e o foco voltar para a janela anterior antes do Ctrl+V."""
    def fazer() -> None:
        try:
            if gravando:
                return
            if atraso:
                time.sleep(atraso)
            itens = historico.listar(limite=1)
            if not itens:
                log("colar último: histórico vazio")
                plat.bipe(300, 150)
                return
            plat.colar(itens[0]["texto"] + " ")
            log("colar último: colado")
        except Exception as e:  # noqa: BLE001 - nunca derruba o app; o motivo vai ao log
            log(f"colar último: falhou ({type(e).__name__})")
            plat.bipe(300, 150)

    threading.Thread(target=fazer, daemon=True).start()


def reiniciar() -> None:
    """Fecha esta instância e abre a versão nova que acabou de ser copiada."""
    import subprocess

    reabrir = ["--janela"] if fechar_janela() else []  # a janela aberta volta já na versão nova
    bandeja.remover()
    plat.liberar_instancia()
    subprocess.Popen([sys.executable, os.path.abspath(__file__), *reabrir], cwd=os.path.dirname(os.path.abspath(__file__)),
                     stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                     start_new_session=sys.platform != "win32")  # não herda o terminal de quem chamou
    sair()


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
    bandeja = plat.Bandeja(ao_sair=sair, ao_atualizar=atualizar, versao=versao,
                           ao_colar=lambda: colar_ultimo(0.35))  # pelo menu: espera o foco voltar
    if not ler_chave():
        bandeja.abrir_configuracoes("key")
    elif "--janela" in sys.argv:  # reiniciado por uma atualização com a janela aberta: reabre em Sobre
        bandeja.abrir_configuracoes("sob")
    threading.Thread(target=ciclo_atualizacao, daemon=True).start()
    threading.Thread(target=ajustes.escrever_dispositivos, daemon=True).start()
    threading.Thread(target=historico.ciclo_retencao, daemon=True).start()  # apaga > 30 dias ao iniciar e 1x por dia
    threading.Thread(target=conferir_modelos, args=(MODELO, escrita.MODELO), daemon=True).start()
    plat.rodar(comecar, parar, marcar_outra_tecla, lambda: gravando, lambda: colar_ultimo(0.1))
