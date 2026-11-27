"""Ajustes do RenanTalk (`ajustes.json` em PASTA): lidos pelo Python a cada ditado, gravados pela janela.

Chaves desconhecidas são preservadas ao salvar (atalhos extras e modos virão depois).
"""
import json
import math
import os
import threading
import time

import numpy as np

from comum import PASTA, log as _log
from escrita import INSTRUCOES

ARQ_AJUSTES = os.path.join(PASTA, "ajustes.json")
ARQ_DISPOSITIVOS = os.path.join(PASTA, "dispositivos.json")
COMANDOS = ("nova_linha", "ponto_final", "virgula", "interrogacao", "abre_parenteses", "fecha_parenteses")  # falados, ver substituicoes.py
PADROES = {"volume": 60, "microfone": "", "idioma": "pt", "atalho_ditar": "ctrl+win", "atalho_colar": "alt+win+v",
           "luz": {"ligada": True, "cor": "azul", "espessura": "media", "posicao": "embaixo"},
           "tema": "sistema", "notificacoes": True, "abaixar_outros": False, "volume_outros": 30,
           "comandos": {c: True for c in COMANDOS},
           "atalho_traduzir": "shift+win", "modo": "literal",
           "modos_apps": {"outlook": "formal", "olk": "formal", "whatsapp": "whatsapp", "code": "literal"}}  # olk = Outlook novo
MODOS = ("literal", *(m for m in INSTRUCOES if m != "ingles"))  # "ingles" só pelo atalho de traduzir; igual ao MODOS do main.rs
ATALHOS = ("atalho_ditar", "atalho_colar", "atalho_traduzir")  # nenhum pode repetir nem conter outro
IDIOMAS = ("pt", "auto", "en")
MODIFICADORES = ("ctrl", "alt", "shift", "win")
TECLAS = tuple("abcdefghijklmnopqrstuvwxyz0123456789") + tuple(f"f{i}" for i in range(1, 13)) + ("space",)
ATALHOS_PROIBIDOS = ("ctrl+c", "ctrl+v", "ctrl+x", "ctrl+z", "ctrl+a", "ctrl+s", "alt+f4",  # o colar manda Ctrl+V
                     "alt+space")  # menu da janela
FREQ_BIPE_TAXA = 44100
TEMAS = ("sistema", "claro", "escuro")
POSICOES = ("embaixo", "topo")
ESPESSURAS = {"fina": 0.5, "media": 1.0, "grossa": 2.0}  # multiplica a altura, o núcleo e o brilho da luz
# RGB 0-255: centro e ponta do núcleo, brilho e névoa (o azul é o original; os outros seguem o mesmo degradê)
CORES_LUZ = {
    "azul": {"centro": (120, 170, 255), "ponta": (37, 99, 235), "brilho": (45, 110, 255), "nevoa": (30, 64, 200)},
    "verde": {"centro": (134, 239, 172), "ponta": (22, 163, 74), "brilho": (34, 197, 94), "nevoa": (21, 128, 61)},
    "roxo": {"centro": (196, 167, 255), "ponta": (124, 58, 237), "brilho": (139, 92, 246), "nevoa": (91, 33, 182)},
    "ambar": {"centro": (253, 224, 130), "ponta": (217, 119, 6), "brilho": (245, 158, 11), "nevoa": (180, 83, 9)},
}

_cache: dict = {"mtime": None, "dados": dict(PADROES)}


# ---------- atalho ----------
def validar_atalho(texto) -> tuple[str, ...] | None:
    """'ctrl+win' -> ('ctrl', 'win'); modificadores em ordem fixa e no máximo uma tecla. None se inválido."""
    if not isinstance(texto, str):
        return None
    partes = [p.strip().lower() for p in texto.split("+")]
    partes = ["win" if p in ("super", "windows", "meta") else p for p in partes]
    if len(partes) < 2 or len(set(partes)) != len(partes):
        return None
    mods = [p for p in partes if p in MODIFICADORES]
    teclas = [p for p in partes if p not in MODIFICADORES]
    # só Shift intercepta a digitação comum (Shift+A): precisa de Ctrl, Alt ou Win
    if len(teclas) > 1 or (teclas and teclas[0] not in TECLAS) or not set(mods) - {"shift"}:
        return None
    saida = tuple(sorted(mods, key=MODIFICADORES.index)) + tuple(teclas)
    # do sistema: Alt+Shift troca o idioma do teclado (mesmo com Win junto) e o Windows usa quase todo Win+tecla
    if "+".join(saida) in ATALHOS_PROIBIDOS or {"alt", "shift"} <= set(mods) or (mods == ["win"] and teclas):
        return None
    return saida


def atalhos_conflitam(a: str, b: str) -> bool:
    """Um atalho contido no outro (ctrl+win e ctrl+win+v) faz um disparar no lugar do outro."""
    ca, cb = set(a.split("+")), set(b.split("+"))
    return ca <= cb or cb <= ca


class Casador:
    """Casa as teclas apertadas com os atalhos. Sem X nem teclado: recebe só nomes ('ctrl', 'win', 'd'...).

    `evento(nome, descer, alvo, gravando, alvo_colar=None, alvo_traduzir=None)` devolve 'iniciar', 'traduzir',
    'parar', 'colar', 'outra' ou None. Traduzir funciona como o de ditar (segure, fale e solte), mas cola em inglês.
    Regra herdada do Ctrl+Win: com o ditado em andamento, qualquer tecla fora do atalho segurado cancela.
    O atalho de colar só vale com o ditado parado e dispara uma vez por vez apertada (sem repetição da tecla).
    """

    def __init__(self) -> None:
        self.apertadas: set[str] = set()
        self._alvos: tuple = ()
        self._colou = False
        self._segurado: tuple = ()  # atalho que começou o ditado em andamento (ditar ou traduzir)

    def evento(self, nome: str, descer: bool, alvo: tuple[str, ...], gravando: bool,
               alvo_colar: tuple[str, ...] | None = None, alvo_traduzir: tuple[str, ...] | None = None) -> str | None:
        if alvo_colar == alvo:  # iguais: o de ditar manda
            alvo_colar = None
        if alvo_traduzir in (alvo, alvo_colar):
            alvo_traduzir = None
        if (alvo, alvo_colar, alvo_traduzir) != self._alvos:  # atalho trocado nas configurações: recomeça do zero
            self._alvos, self.apertadas, self._colou = (alvo, alvo_colar, alvo_traduzir), set(), False
        colar, traduzir = alvo_colar or (), alvo_traduzir or ()
        if descer:
            if nome not in alvo + colar + traduzir and nome not in MODIFICADORES:
                return "outra" if gravando else None
            self.apertadas.add(nome)
            if gravando:
                return None if nome in self._segurado else "outra"
            if self.apertadas == set(alvo):
                self._segurado = alvo
                return "iniciar"
            if traduzir and self.apertadas == set(traduzir):
                self._segurado = traduzir
                return "traduzir"
            if colar and self.apertadas == set(colar) and not self._colou:
                self._colou = True
                return "colar"
            return None
        self.apertadas.discard(nome)
        if nome in colar:
            self._colou = False
        return "parar" if gravando and nome in self._segurado else None

    def esperar_soltar(self, limite: float = 2.0) -> None:
        """Espera as teclas do atalho serem soltas (ou o limite): colar com Alt/Win apertados vira outro atalho."""
        fim = time.time() + limite
        while self.apertadas and time.time() < fim:
            time.sleep(0.02)


def colar_ao_soltar(casador: Casador, ao_colar) -> None:
    """Atalho de colar: espera soltar as teclas (numa thread, para não travar o listener) e só então cola."""
    def esperar_e_colar() -> None:
        casador.esperar_soltar()
        ao_colar()

    threading.Thread(target=esperar_e_colar, daemon=True).start()


# ---------- leitura e gravação ----------
def _normalizar_luz(bruto) -> dict:
    """Luz com os quatro campos válidos; o que faltar ou for inválido volta ao padrão (chaves extras ficam)."""
    luz = dict(bruto) if isinstance(bruto, dict) else {}
    padrao = PADROES["luz"]
    if not isinstance(luz.get("ligada"), bool):
        luz["ligada"] = padrao["ligada"]
    if luz.get("cor") not in CORES_LUZ:
        luz["cor"] = padrao["cor"]
    if luz.get("espessura") not in ESPESSURAS:
        luz["espessura"] = padrao["espessura"]
    if luz.get("posicao") not in POSICOES:
        luz["posicao"] = padrao["posicao"]
    return luz


def _normalizar_comandos(bruto) -> dict:
    """Liga/desliga dos comandos falados: o que faltar ou não for booleano fica ligado (chaves extras ficam)."""
    comandos = dict(bruto) if isinstance(bruto, dict) else {}
    for c in COMANDOS:
        if not isinstance(comandos.get(c), bool):
            comandos[c] = True
    return comandos


def normalizar(bruto) -> dict:
    """Mescla com os padrões e troca valor inválido pelo padrão (a chave desconhecida segue junto)."""
    dados = dict(bruto) if isinstance(bruto, dict) else {}
    for chave, padrao in PADROES.items():
        dados.setdefault(chave, dict(padrao) if isinstance(padrao, dict) else padrao)
    for chave in ("volume", "volume_outros"):
        v = dados[chave]
        ok = isinstance(v, (int, float)) and not isinstance(v, bool) and math.isfinite(v)  # json aceita NaN e Infinity
        dados[chave] = max(0, min(100, int(v))) if ok else PADROES[chave]
    if not isinstance(dados["microfone"], str):
        dados["microfone"] = ""
    if dados["idioma"] not in IDIOMAS:
        dados["idioma"] = PADROES["idioma"]
    for i, chave in enumerate(ATALHOS):  # cada um é conferido contra os anteriores, já válidos
        anteriores = [dados[c] for c in ATALHOS[:i] if dados[c]]
        for candidato in (dados[chave], PADROES[chave]):
            t = validar_atalho(candidato)
            if t and not any(atalhos_conflitam("+".join(t), o) for o in anteriores):
                break
            _log(f"ajustes: {chave} inválido ou em conflito com outro atalho {candidato!r}")
        else:
            t = ()  # nem o padrão serve (o de ditar ocupou o padrão do colar ou do traduzir): fica desligado
        dados[chave] = "+".join(t)
    if dados["modo"] not in MODOS:
        dados["modo"] = PADROES["modo"]
    apps = dados["modos_apps"] if isinstance(dados["modos_apps"], dict) else {}
    dados["modos_apps"] = {k.strip().casefold(): v for k, v in apps.items()
                           if isinstance(k, str) and k.strip() and v in MODOS}
    dados["luz"] = _normalizar_luz(dados["luz"])
    dados["comandos"] = _normalizar_comandos(dados["comandos"])
    if dados["tema"] not in TEMAS:
        dados["tema"] = PADROES["tema"]
    for chave in ("notificacoes", "abaixar_outros"):
        if not isinstance(dados[chave], bool):
            dados[chave] = PADROES[chave]
    return dados


def ler() -> dict:
    """Ajustes atuais; relê o arquivo só quando o mtime muda. Ausente ou corrompido: padrões."""
    try:
        mtime = os.stat(ARQ_AJUSTES).st_mtime_ns
    except OSError:
        mtime = None
    if mtime == _cache["mtime"]:
        return _cache["dados"]
    bruto = {}
    if mtime is not None:
        try:
            with open(ARQ_AJUSTES, encoding="utf-8") as f:
                bruto = json.load(f)
            if not isinstance(bruto, dict):
                raise ValueError("não é um objeto")
        except (OSError, ValueError) as e:
            _log(f"ajustes: {ARQ_AJUSTES} ilegível ({e}), usando os padrões")
            bruto = {}
    _cache["mtime"], _cache["dados"] = mtime, normalizar(bruto)
    return _cache["dados"]


def salvar(novos: dict) -> dict:
    """Grava só as chaves recebidas, mantendo as demais do arquivo. Escrita atômica (temporário + rename)."""
    try:
        with open(ARQ_AJUSTES, encoding="utf-8") as f:
            atual = json.load(f)
        if not isinstance(atual, dict):
            atual = {}
    except (OSError, ValueError):
        atual = {}
    atual.update(novos)
    dados = normalizar(atual)
    _gravar_json(ARQ_AJUSTES, dados)
    return dados


def _gravar_json(arquivo: str, dados) -> None:
    """Escrita atômica (temporário + rename): quem lê nunca vê o arquivo pela metade."""
    tmp = arquivo + ".tmp"
    with open(tmp, "w", encoding="utf-8") as f:
        json.dump(dados, f, ensure_ascii=False, indent=2)
    os.replace(tmp, arquivo)


# ---------- som ----------
def onda_bipe(freq: int, ms: int, volume: int, taxa: int = FREQ_BIPE_TAXA) -> np.ndarray:
    """Seno com envelope de 8 ms; o pico é volume/100 do máximo (volume 30 -> 0,3)."""
    t = np.arange(int(taxa * ms / 1000)) / taxa
    env = np.minimum(1, np.minimum(t, t[::-1]) / 0.008)  # sem estalo no começo e no fim
    return (max(0, min(100, volume)) / 100 * np.sin(2 * np.pi * freq * t) * env).astype(np.float32)


def bipe(freq: int, ms: int) -> None:
    """Toca o bipe no volume dos ajustes; volume 0 = mudo."""
    volume = ler()["volume"]
    if volume <= 0:
        return
    try:
        import sounddevice as sd

        sd.play(onda_bipe(freq, ms, volume), FREQ_BIPE_TAXA)
    except Exception:  # noqa: BLE001 - sem saída de áudio não é motivo para travar o ditado
        pass


# ---------- microfone ----------
def listar_entradas() -> list[dict]:
    """Dispositivos de entrada sem nome repetido; marca o padrão do sistema."""
    import sounddevice as sd

    try:
        padrao = sd.query_devices(kind="input")["name"]
    except Exception:  # noqa: BLE001
        padrao = ""
    vistos, lista = set(), []
    for d in sd.query_devices():
        if d["max_input_channels"] > 0 and d["name"] not in vistos and _abre_a_16k(d):
            vistos.add(d["name"])
            lista.append({"nome": d["name"], "padrao": d["name"] == padrao})
    return lista


def _abre_a_16k(d: dict) -> bool:
    """No Windows, entradas WDM-KS e saídas listadas como entrada não abrem a 16 kHz: ficam fora da lista."""
    import sounddevice as sd

    try:
        sd.check_input_settings(device=d["index"], channels=1, dtype="int16", samplerate=16000)
        return True
    except Exception:  # noqa: BLE001
        return False


def escrever_dispositivos() -> None:
    """Grava `dispositivos.json` para a janela (que é Rust e não enxerga o sounddevice)."""
    try:
        _gravar_json(ARQ_DISPOSITIVOS, {"dispositivos": listar_entradas()})
    except Exception as e:  # noqa: BLE001
        _log(f"dispositivos: não consegui listar ({e!r})")


def indice_do_microfone(nome: str, dispositivos: list[dict]) -> int | None:
    """Índice do dispositivo de entrada com esse nome; None = padrão do sistema (nome vazio ou sumiu)."""
    if not nome:
        return None
    for i, d in enumerate(dispositivos):
        if d["name"] == nome and d["max_input_channels"] > 0:
            return i
    _log(f"microfone {nome!r} não encontrado, usando o padrão do sistema")
    return None
