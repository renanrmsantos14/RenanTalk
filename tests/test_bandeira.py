import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "app"))
import bandeira  # noqa: E402


class Bandeira(unittest.TestCase):
    def test_desenho(self):
        f = bandeira.eua()
        self.assertEqual(f.shape, (bandeira.ALTURA_PX, bandeira.LARGURA_PX, 4))
        self.assertLessEqual(f[..., 3].max(), bandeira.OPACIDADE + 1e-6)  # discreta: nunca opaca
        self.assertTrue((f[..., :3] <= f[..., 3:] + 1e-6).all())  # cor pré-multiplicada
        canto, listra = f[2, 3], f[8, 20]  # azul no alto à esquerda; vermelho na 5ª listra
        self.assertGreater(canto[2], canto[0])
        self.assertGreater(listra[0], listra[2])

    def test_cabe_na_luz_em_toda_espessura(self):
        for k in (0.5, 1.0, 2.0):
            self.assertLessEqual(bandeira.distancia_da_borda(k) + bandeira.ALTURA_PX, round(36 * k))


if __name__ == "__main__":
    unittest.main()
