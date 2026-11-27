"""Confere assinaturas minisign (as do Tauri: `.sig` do instalador e `pubkey` do tauri.conf.json), só com a stdlib.

Ed25519 pela referência da RFC 8032 (seção 6): é lento para assinar, mas verificar uma vez leva milissegundos,
e não precisa instalar nada (o pip na máquina do usuário é o que mais falha numa atualização).
Formato minisign: https://jedisct1.github.io/minisign/ (chave "Ed" + id de 8 bytes + 32 bytes; assinatura "ED"
= Ed25519 do BLAKE2b-512 do arquivo, ou "Ed" = do arquivo inteiro; mais a assinatura global do comentário confiável).
"""
import base64
import hashlib

_P = 2**255 - 19
_Q = 2**252 + 27742317777372353535851937790883648493
_D = -121665 * pow(121666, _P - 2, _P) % _P
_RAIZ_M1 = pow(2, (_P - 1) // 4, _P)


class AssinaturaInvalida(Exception):
    pass


def _somar(a, b):
    x = (a[1] - a[0]) * (b[1] - b[0]) % _P
    y = (a[1] + a[0]) * (b[1] + b[0]) % _P
    c = 2 * a[3] * b[3] * _D % _P
    d = 2 * a[2] * b[2] % _P
    e, f, g, h = y - x, d - c, d + c, y + x
    return (e * f % _P, g * h % _P, f * g % _P, e * h % _P)


def _vezes(s: int, p):
    r = (0, 1, 1, 0)
    while s:
        if s & 1:
            r = _somar(r, p)
        p = _somar(p, p)
        s >>= 1
    return r


def _iguais(a, b) -> bool:
    return (a[0] * b[2] - b[0] * a[2]) % _P == 0 and (a[1] * b[2] - b[1] * a[2]) % _P == 0


def _x(y: int, sinal: int):
    if y >= _P:
        return None
    x2 = (y * y - 1) * pow(_D * y * y + 1, _P - 2, _P) % _P
    if x2 == 0:
        return None if sinal else 0
    x = pow(x2, (_P + 3) // 8, _P)
    if (x * x - x2) % _P:
        x = x * _RAIZ_M1 % _P
    if (x * x - x2) % _P:
        return None
    return _P - x if (x & 1) != sinal else x


def _ponto(b: bytes):
    y = int.from_bytes(b, "little")
    sinal, y = y >> 255, y & ((1 << 255) - 1)
    x = _x(y, sinal)
    return None if x is None else (x, y, 1, x * y % _P)


_GY = 4 * pow(5, _P - 2, _P) % _P
_G = (_x(_GY, 0), _GY, 1, _x(_GY, 0) * _GY % _P)


def ed25519_confere(chave: bytes, msg: bytes, assinatura: bytes) -> bool:
    if len(chave) != 32 or len(assinatura) != 64:
        return False
    a, r = _ponto(chave), _ponto(assinatura[:32])
    s = int.from_bytes(assinatura[32:], "little")
    if a is None or r is None or s >= _Q:
        return False
    h = int.from_bytes(hashlib.sha512(assinatura[:32] + chave + msg).digest(), "little") % _Q
    return _iguais(_vezes(s, _G), _somar(r, _vezes(h, a)))


def _linhas(texto_b64: str) -> list[str]:
    """O Tauri guarda a chave e o .sig em base64 do texto do minisign (o latest.json também)."""
    try:
        texto = base64.b64decode(texto_b64.strip(), validate=True).decode("utf-8")
    except Exception as e:  # noqa: BLE001
        raise AssinaturaInvalida(f"base64 inválido: {e}") from e
    return [l.rstrip("\r") for l in texto.split("\n") if l.strip()]


def ler_chave(pubkey_b64: str) -> tuple[bytes, bytes]:
    """(id, chave) da `pubkey` do tauri.conf.json."""
    linhas = _linhas(pubkey_b64)
    bruto = base64.b64decode(linhas[-1])
    if len(bruto) != 42 or bruto[:2] != b"Ed":
        raise AssinaturaInvalida("chave pública não é minisign Ed25519")
    return bruto[2:10], bruto[10:]


def conferir(dados: bytes, assinatura_b64: str, pubkey_b64: str) -> str:
    """Levanta AssinaturaInvalida se `dados` não foram assinados pela chave. Devolve o comentário confiável."""
    ident, chave = ler_chave(pubkey_b64)
    linhas = _linhas(assinatura_b64)
    if len(linhas) < 4 or not linhas[2].startswith("trusted comment: "):
        raise AssinaturaInvalida("assinatura fora do formato minisign")
    bruto, global_ = base64.b64decode(linhas[1]), base64.b64decode(linhas[3])
    if len(bruto) != 74:
        raise AssinaturaInvalida("assinatura com tamanho errado")
    alg, ident_sig, sig = bruto[:2], bruto[2:10], bruto[10:]
    if ident_sig != ident:
        raise AssinaturaInvalida("assinada por outra chave")
    if alg == b"ED":
        msg = hashlib.blake2b(dados, digest_size=64).digest()
    elif alg == b"Ed":
        msg = dados
    else:
        raise AssinaturaInvalida(f"algoritmo desconhecido {alg!r}")
    if not ed25519_confere(chave, msg, sig):
        raise AssinaturaInvalida("o arquivo não confere com a assinatura")
    comentario = linhas[2][len("trusted comment: "):]
    if not ed25519_confere(chave, sig + comentario.encode("utf-8"), global_):
        raise AssinaturaInvalida("comentário confiável adulterado")
    return comentario
