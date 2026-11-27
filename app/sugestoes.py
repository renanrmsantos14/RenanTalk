"""Sugestões de termos para o vocabulário, a partir da frequência nos ditados (tabela `sugestao` de historico.db).

Candidato: palavra com maiúscula fora do início de frase (palavras assim em sequência viram um termo só), ou sigla (2+ maiúsculas), com 5 ocorrências ou mais nos
ditados da janela. Ficam de fora o que já está no vocabulário (cada palavra, inteira ou como parte de um termo composto,
sem diferenciar maiúsculas) e as palavras comuns abaixo. Aceita ou ignorada nunca volta a `nova`; só a contagem acompanha.
"""
import re
from collections import Counter

MINIMO = 5
# pt-BR comum que aparece com maiúscula no meio da frase (depois de ":" ou em título) e não merece sugestão
COMUNS = frozenset("""
eu você voce não nao sim olá ola oi bom boa dia tarde noite obrigado obrigada por favor então entao mas e ou que para
com de da do em na no um uma os as tudo bem ok tá ta né ne também ainda hoje amanhã ontem segunda terça quarta quinta
sexta sábado sabado domingo pode vai vamos aqui ali quando como porque se já agora depois antes ele ela nós eles isso
isto esse essa este esta o a meu minha seu sua qual quem onde mais muito pouco todo toda cada outro outra
""".split())
# fim de frase ou quebra de linha: a palavra seguinte abre frase e não conta
_TOKEN = re.compile(r"([.!?…]+|\n)|([^\W\d_][\w'’-]*)")


def candidatos(textos) -> dict[str, Counter]:
    """chave minúscula -> formas escritas e quantas vezes cada uma apareceu como candidata.

    Palavras com maiúscula seguidas, separadas só por espaço, formam um termo só ("Santos Dumont")."""
    formas: dict[str, Counter] = {}

    def fechar(seq):
        if seq:
            termo = " ".join(seq)
            formas.setdefault(termo.casefold(), Counter())[termo] += 1

    for texto in textos:
        inicio, seq, fim = True, [], 0
        for m in _TOKEN.finditer(texto):
            palavra = m.group(2) and m.group(2).rstrip("'’-")
            if palavra and not inicio and len(palavra) >= 2 and palavra[0].isupper() and palavra.casefold() not in COMUNS:
                if not texto[fim:m.start()].isspace():
                    fechar(seq)
                    seq = []
                seq.append(palavra)
            else:
                fechar(seq)
                seq = []
            inicio = bool(m.group(1))
            fim = m.end()
        fechar(seq)
    return formas


def atualizar(con, vocab: list[str], corte: str) -> None:
    """Conta os candidatos nos ditados desde `corte` (ISO) e grava/atualiza a tabela `sugestao`. Não faz commit do resto."""
    achados = candidatos(l[0] for l in con.execute("SELECT texto FROM ditado WHERE criado_em >= ? AND modo != 'ingles'", (corte,)))
    conhecidos = {p.casefold() for t in vocab for p in (t, *t.split())}
    existentes = {t.casefold(): (t, n) for t, n in con.execute("SELECT termo, contagem FROM sugestao")}
    for chave, formas in achados.items():
        total = sum(formas.values())
        if chave in existentes:
            termo, antes = existentes[chave]
            if total != antes:
                con.execute("UPDATE sugestao SET contagem=? WHERE termo=?", (total, termo))
        elif total >= MINIMO and not set(chave.split()) <= conhecidos:
            con.execute("INSERT INTO sugestao(termo, motivo, contagem, estado) VALUES(?, 'frequencia', ?, 'nova')",
                        (formas.most_common(1)[0][0], total))
    # sugestão ainda pendente cujos ditados saíram da janela: some (a contagem "nos últimos 30 dias" ficaria falsa)
    for chave, (termo, _) in existentes.items():
        if chave not in achados:
            con.execute("DELETE FROM sugestao WHERE termo=? AND estado='nova' AND motivo='frequencia'", (termo,))
    con.commit()
