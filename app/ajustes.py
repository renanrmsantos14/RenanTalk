"""Ajustes do RenanTalk (`ajustes.json` em PASTA): lidos pelo Python a cada ditado, gravados pela janela.

Chaves desconhecidas são preservadas ao salvar (luz, tema, notificações, atalhos extras, modos virão depois).
"""
import json
import os
import time

import numpy as np

from comum import ARQ_LOG, PASTA

ARQ_AJUSTES = os.path.join(PASTA, "ajustes.json")
ARQ_DISPOSITIVOS = os.path.join(PASTA, "dispositivos.json")
PADROES = {"volume": 60, "microfone": "", "idioma": "pt", "atalho_ditar": "ctrl+win"}
IDIOMAS = ("pt", "auto", "en")
MODIFICADORES = ("ctrl", "alt", "shift", "win")
TECLAS = tuple("abcdefghijklmnopqrstuvwxyz0123456789") + tuple(f"f{i}" for i in range(1, 13)) + ("space",)
FREQ_BIPE_TAXA = 44100

_cache: dict = {"mtime": None, "dados": dict(PADROES)}


def _log(msg: str) -> None:
    try:
        with open(ARQ_LOG, "a", encoding="utf-8") as f:
            f.write(time.strftime("%d/%m/%Y %H:%M:%S ") + msg + "\n")
    except OSError:
        pass


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
    if len(teclas) > 1 or (teclas and teclas[0] not in TECLAS) or not mods:
        return None
    return tuple(sorted(mods, key=MODIFICADORES.index)) + tuple(teclas)


class Casador:
    """Casa as teclas apertadas com o atalho. Sem X nem teclado: recebe só nomes ('ctrl', 'win', 'd'...).

    `evento(nome, descer, alvo, gravando)` devolve 'iniciar', 'parar', 'outra' ou None.
    Regra herdada do Ctrl+Win: com o ditado em andamento, qualquer tecla fora do atalho cancela.
    """

    def __init__(self) -> None:
        self.apertadas: set[str] = set()
        self._alvo: tuple[str, ...] = ()

    def evento(self, nome: str, descer: bool, alvo: tuple[str, ...], gravando: bool) -> str | None:
        if alvo != self._alvo:  # atalho trocado nas configurações: recomeça do zero
            self._alvo, self.apertadas = alvo, set()
        if descer:
            if nome not in alvo and nome not in MODIFICADORES:
                return "outra" if gravando else None
            self.apertadas.add(nome)
            if gravando:
                return None if nome in alvo else "outra"
            return "iniciar" if self.apertadas == set(alvo) else None
        self.apertadas.discard(nome)
        return "parar" if gravando and nome in alvo else None


# ---------- leitura e gravação ----------
def normalizar(bruto) -> dict:
    """Mescla com os padrões e troca valor inválido pelo padrão (a chave desconhecida segue junto)."""
    dados = dict(bruto) if isinstance(bruto, dict) else {}
    for chave, padrao in PADROES.items():
        dados.setdefault(chave, padrao)
    v = dados["volume"]
    dados["volume"] = max(0, min(100, int(v))) if isinstance(v, (int, float)) and not isinstance(v, bool) else PADROES["volume"]
    if not isinstance(dados["microfone"], str):
        dados["microfone"] = ""
    if dados["idioma"] not in IDIOMAS:
        dados["idioma"] = PADROES["idioma"]
    if validar_atalho(dados["atalho_ditar"]) is None:
        _log(f"ajustes: atalho inválido {dados['atalho_ditar']!r}, usando {PADROES['atalho_ditar']}")
        dados["atalho_ditar"] = PADROES["atalho_ditar"]
    else:
        dados["atalho_ditar"] = "+".join(validar_atalho(dados["atalho_ditar"]))
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
    tmp = ARQ_AJUSTES + ".tmp"
    with open(tmp, "w", encoding="utf-8") as f:
        json.dump(dados, f, ensure_ascii=False, indent=2)
    os.replace(tmp, ARQ_AJUSTES)
    return dados


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
        if d["max_input_channels"] > 0 and d["name"] not in vistos:
            vistos.add(d["name"])
            lista.append({"nome": d["name"], "padrao": d["name"] == padrao})
    return lista


def escrever_dispositivos() -> None:
    """Grava `dispositivos.json` para a janela (que é Rust e não enxerga o sounddevice)."""
    try:
        tmp = ARQ_DISPOSITIVOS + ".tmp"
        with open(tmp, "w", encoding="utf-8") as f:
            json.dump({"dispositivos": listar_entradas()}, f, ensure_ascii=False)
        os.replace(tmp, ARQ_DISPOSITIVOS)
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
