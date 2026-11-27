import os
import subprocess
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "app"))
import comum  # noqa: E402


class FecharJanela(unittest.TestCase):
    def tearDown(self):
        comum._janela = None

    def test_fecha_a_aberta_uma_vez(self):
        comum._janela = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(30)"])
        self.assertTrue(comum.fechar_janela())
        comum._janela.wait(timeout=5)
        self.assertFalse(comum.fechar_janela())  # já fechada: o ditado novo não reabre à toa

    def test_sem_janela(self):
        self.assertFalse(comum.fechar_janela())


if __name__ == "__main__":
    unittest.main()
