"""Histórico de ditados (`historico.db` em PASTA, SQLite em modo WAL).

O Python grava; a janela (Rust, `rusqlite`) lê e edita. Itens com mais de 30 dias são apagados.
Gravar nunca pode impedir o texto de ser colado: `gravar` captura qualquer erro e só registra no log (sem conteúdo).
"""
import os
import sqlite3
import time
from datetime import datetime, timedelta

from comum import ARQ_LOG, PASTA

ARQ_HISTORICO = os.path.join(PASTA, "historico.db")
VERSAO = 1
RETENCAO_DIAS = 30
TIMEOUT_S = 2.0  # banco travado por outro processo: espera pouco e desiste
_FORMATO = "%Y-%m-%dT%H:%M:%S"


def _log(msg: str) -> None:
    try:
        with open(ARQ_LOG, "a", encoding="utf-8") as f:
            f.write(time.strftime("%d/%m/%Y %H:%M:%S ") + msg + "\n")
    except OSError:
        pass


def conectar(arquivo: str | None = None) -> sqlite3.Connection:
    """Abre (criando, se preciso) o banco e garante o schema."""
    arq = arquivo or ARQ_HISTORICO
    os.makedirs(os.path.dirname(arq), exist_ok=True)
    con = sqlite3.connect(arq, timeout=TIMEOUT_S)
    con.row_factory = sqlite3.Row
    con.execute("PRAGMA journal_mode=WAL")
    if con.execute("PRAGMA user_version").fetchone()[0] < VERSAO:
        con.executescript(
            """
            CREATE TABLE IF NOT EXISTS ditado(
                id INTEGER PRIMARY KEY,
                criado_em TEXT NOT NULL,
                texto TEXT NOT NULL,
                original TEXT NOT NULL,
                app TEXT NOT NULL DEFAULT '',
                modo TEXT NOT NULL DEFAULT 'literal',
                idioma TEXT NOT NULL DEFAULT '',
                duracao_s REAL NOT NULL DEFAULT 0,
                palavras INTEGER NOT NULL DEFAULT 0,
                editado INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS ix_ditado_criado_em ON ditado(criado_em);
            CREATE TABLE IF NOT EXISTS sugestao(
                termo TEXT PRIMARY KEY,
                motivo TEXT NOT NULL DEFAULT '',
                contagem INTEGER NOT NULL DEFAULT 0,
                estado TEXT NOT NULL DEFAULT 'nova'
            );
            """
        )
        con.execute(f"PRAGMA user_version={VERSAO}")
        con.commit()
    return con


def gravar(texto: str, app: str = "", modo: str = "literal", idioma: str = "", duracao_s: float = 0.0,
           original: str | None = None, arquivo: str | None = None) -> int | None:
    """Guarda um ditado. Devolve o id, ou None se falhar (o erro vai ao log, sem o texto)."""
    try:
        con = conectar(arquivo)
        try:
            cur = con.execute(
                "INSERT INTO ditado(criado_em, texto, original, app, modo, idioma, duracao_s, palavras, editado)"
                " VALUES(?,?,?,?,?,?,?,?,0)",
                (datetime.now().strftime(_FORMATO), texto, texto if original is None else original, app or "",
                 modo, idioma or "", round(float(duracao_s), 2), len(texto.split())),
            )
            con.commit()
            return cur.lastrowid
        finally:
            con.close()
    except Exception as e:  # noqa: BLE001 - histórico é secundário: o texto precisa ser colado de qualquer jeito
        _log(f"histórico: não gravou ({type(e).__name__})")
        return None


def listar(busca: str = "", limite: int = 500, arquivo: str | None = None) -> list[dict]:
    con = conectar(arquivo)
    try:
        like = "%" + busca.replace("\\", "\\\\").replace("%", "\\%").replace("_", "\\_") + "%"
        linhas = con.execute(
            "SELECT * FROM ditado WHERE texto LIKE ? ESCAPE '\\' ORDER BY criado_em DESC, id DESC LIMIT ?",
            (like, limite),
        ).fetchall()
        return [dict(l) for l in linhas]
    finally:
        con.close()


def editar(id_: int, texto: str, arquivo: str | None = None) -> bool:
    """Corrige o texto: marca editado=1 e preserva `original`."""
    con = conectar(arquivo)
    try:
        cur = con.execute("UPDATE ditado SET texto=?, palavras=?, editado=1 WHERE id=?",
                          (texto, len(texto.split()), id_))
        con.commit()
        return cur.rowcount > 0
    finally:
        con.close()


def excluir(id_: int, arquivo: str | None = None) -> bool:
    con = conectar(arquivo)
    try:
        cur = con.execute("DELETE FROM ditado WHERE id=?", (id_,))
        con.commit()
        return cur.rowcount > 0
    finally:
        con.close()


def apagar_tudo(arquivo: str | None = None) -> int:
    con = conectar(arquivo)
    try:
        cur = con.execute("DELETE FROM ditado")
        con.commit()
        return cur.rowcount
    finally:
        con.close()


def limpar_antigos(dias: int = RETENCAO_DIAS, arquivo: str | None = None) -> int:
    """Apaga ditados com mais de `dias` dias. Nunca propaga erro (roda em thread de fundo)."""
    try:
        corte = (datetime.now() - timedelta(days=dias)).strftime(_FORMATO)
        con = conectar(arquivo)
        try:
            cur = con.execute("DELETE FROM ditado WHERE criado_em < ?", (corte,))
            con.commit()
            return cur.rowcount
        finally:
            con.close()
    except Exception as e:  # noqa: BLE001
        _log(f"histórico: limpeza falhou ({type(e).__name__})")
        return 0


def ciclo_retencao() -> None:
    """Para thread daemon: limpa ao iniciar e depois uma vez por dia."""
    while True:
        limpar_antigos()
        time.sleep(24 * 3600)
