import os
import re
import unittest

UI = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "app", "ui", "index.html")
# rolam por dentro (lista ou chips com overflow próprio) ou cabem sempre na janela
SEM_ROLAGEM_PROPRIA = {"hist", "voc", "key", "sob"}


class Abas(unittest.TestCase):
    def test_toda_aba_rola(self):
        """`main` tem overflow:hidden: aba nova fora da regra .pane#x{overflow:auto} fica cortada (aconteceu 2x)."""
        with open(UI, encoding="utf-8") as f:
            html = f.read()
        abas = set(re.findall(r'<section[^>]*\bclass="pane\b[^>]*\bid="(\w+)"', html))
        regra = re.search(r"((?:\.pane#\w+,?)+)\{overflow:auto", html)
        self.assertTrue(abas and regra, "formato do index.html mudou: ajuste este teste")
        rolam = set(re.findall(r"#(\w+)", regra.group(1)))
        self.assertEqual(abas - rolam - SEM_ROLAGEM_PROPRIA, set(),
                         "aba sem rolagem: inclua em .pane#...{overflow:auto} ou em SEM_ROLAGEM_PROPRIA")


if __name__ == "__main__":
    unittest.main()
