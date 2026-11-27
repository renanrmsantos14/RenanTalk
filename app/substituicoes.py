"""Substituições no texto ditado: comandos falados prontos e regras próprias (`substituicoes.txt` em PASTA).

Arquivo: uma regra `quando eu disser => escreva` por linha; `#` comenta; `\\n` no lado direito vira quebra de linha.
Sem diferenciar maiúsculas e só em palavra inteira. Regra própria vale antes dos comandos prontos.

Comandos prontos (ligados e desligados em ajustes.json, chave `comandos`): o Whisper já põe pontuação ao redor do que
ouve ("Nova linha.", "vírgula,"); ela é tirada. A pontuação colada à palavra anterior sai, o espaço some antes de
`. , ? )` e depois de `(`, e a palavra seguinte a "ponto final" e "interrogação" ganha maiúscula. Não faz: maiúscula
depois de "nova linha" nem concordância; "nova linha" dentro de uma frase comum também vira quebra (desligue o comando).
"""
import os
import re

from comum import PASTA

ARQ_SUBST = os.path.join(PASTA, "substituicoes.txt")
_SINAIS = ".!?…"
# id: (frases, saída, pontuação anterior que se mantém, pontuação posterior que se mantém).
# {antes}/{depois} é a pontuação que o Whisper pôs ao redor e que vale manter; \0 marca onde a próxima palavra ganha maiúscula.
_COMANDOS = {
    "nova_linha": (r"nova\s+linha", "{antes}\n", _SINAIS, ""),
    "ponto_final": (r"ponto\s+final", ".\0 ", "", ""),
    "virgula": (r"v[ií]rgula", ", ", "", ""),
    "interrogacao": (r"(?:ponto\s+de\s+)?interroga[cç][aã]o", "?\0 ", "", ""),
    "abre_parenteses": (r"(?:abre|abrir)\s+par[eê]nteses?", "{antes} (", _SINAIS, ""),
    "fecha_parenteses": (r"(?:fecha|fechar)\s+par[eê]nteses?", "){depois} ", "", _SINAIS),
}


def ler_regras(arquivo: str | None = None) -> list[tuple[str, str]]:
    """Pares (quando, escreva) do arquivo; ausente ou linha sem `=>` é ignorado."""
    try:
        with open(arquivo or ARQ_SUBST, encoding="utf-8") as f:
            linhas = f.read().splitlines()
    except OSError:
        return []
    regras = []
    for linha in linhas:
        quando, sep, escreva = linha.partition("=>")
        quando, escreva = quando.strip(), escreva.strip()
        if sep and quando and not quando.startswith("#") and escreva:
            regras.append((quando, escreva.replace("\\n", "\n")))
    return regras


def _frase(texto: str) -> str:
    return r"\s+".join(re.escape(p) for p in texto.split())


def aplicar(texto: str, comandos: dict, regras: list[tuple[str, str]] | None = None) -> str:
    """Texto com as regras próprias e os comandos ligados aplicados. Sem nada a trocar, devolve o texto como veio."""
    regras = ler_regras() if regras is None else regras
    if regras:
        mapa = {" ".join(q.lower().split()): e for q, e in regras}
        achar = "|".join(_frase(q) for q in sorted(mapa, key=len, reverse=True))
        texto = re.sub(rf"(?<!\w)(?:{achar})(?!\w)", lambda m: mapa[" ".join(m.group().lower().split())], texto,
                       flags=re.IGNORECASE)
    ativos = {c: _COMANDOS[c] for c in _COMANDOS if comandos.get(c, True)}
    if not ativos:
        return texto
    partes = [f"(?P<{c}>{frase})" for c, (frase, *_) in ativos.items()]
    padrao = re.compile(rf"(?P<antes>[,;:.!?…]?)[ \t]*(?<!\w)(?:{'|'.join(partes)})(?!\w)(?P<depois>[.,;:!?…]*)[ \t]*",
                        re.IGNORECASE)

    def trocar(m: re.Match) -> str:
        c = next(k for k in ativos if m.group(k))
        _, saida, mantem_antes, mantem_depois = ativos[c]
        antes = m.group("antes") if m.group("antes") in mantem_antes and m.group("antes") else ""
        depois = "".join(ch for ch in m.group("depois") if ch in mantem_depois)
        return saida.format(antes=antes, depois=depois)

    texto, n = padrao.subn(trocar, texto)
    if not n:
        return texto
    texto = re.sub(r"[ \t]+\n", "\n", texto)
    texto = re.sub(r"\n[ \t]+", "\n", texto)
    texto = re.sub(r"\0([ \n]*)(\w)", lambda m: m.group(1) + m.group(2).upper(), texto)
    return re.sub(r" {2,}", " ", texto.replace("\0", "")).strip(" ")
