"""Bandeira dos EUA pequena e discreta, desenhada sobre a luz no ditado em inglês (Shift+Win)."""
import numpy as np

ALTURA_PX, LARGURA_PX = 12, 23  # ~19:10
OPACIDADE = 0.85


def eua() -> np.ndarray:
    """RGBA float 0..1, cor pré-multiplicada, (12, 23, 4). Desenhada 4x maior e reduzida (bordas suaves)."""
    s = 4
    h, w = ALTURA_PX * s, LARGURA_PX * s
    y, x = np.mgrid[0:h, 0:w].astype(np.float32)
    listra = (y * 7 / h).astype(int)  # 7 listras: as 13 reais viram rosa em 12 px
    rgb = np.where((listra % 2 == 0)[..., None], [0.70, 0.13, 0.20], [1.0, 1.0, 1.0]).astype(np.float32)
    ch, cw = h * 4 / 7, w * 0.4
    canto = (y < ch) & (x < cw)
    rgb[canto] = [0.24, 0.23, 0.43]
    # estrelas como pontos: 3 linhas x 4 colunas, legíveis nesse tamanho
    ey, ex = (y / ch * 3) % 1, (x / cw * 4) % 1
    estrela = canto & ((ey - 0.5) ** 2 + (ex - 0.5) ** 2 < 0.06)
    rgb[estrela] = [1.0, 1.0, 1.0]
    r = 2 * s  # cantos arredondados
    dx, dy = np.maximum(np.maximum(r - x, x - (w - 1 - r)), 0), np.maximum(np.maximum(r - y, y - (h - 1 - r)), 0)
    a = (dx ** 2 + dy ** 2 <= r * r).astype(np.float32)
    borda = (dx ** 2 + dy ** 2 > (r - s) ** 2) | (x < s) | (x >= w - s) | (y < s) | (y >= h - s)
    rgb[borda] *= 0.55  # contorno escuro de 1 px: a listra branca não some em fundo claro
    img = np.concatenate([rgb * a[..., None], a[..., None]], axis=2)
    return img.reshape(ALTURA_PX, s, LARGURA_PX, s, 4).mean(axis=(1, 3)) * OPACIDADE


def distancia_da_borda(k: float) -> int:
    """Pixels entre a borda da tela e a base da bandeira: acima do núcleo da luz, em qualquer espessura."""
    return round(2 * k) + 4
