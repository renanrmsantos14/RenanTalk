"""Testes das substituições de texto. Rodar: python3 -m unittest discover -s tests"""
import os
import sys
import tempfile
import unittest

TMP = tempfile.mkdtemp()
os.environ["XDG_CONFIG_HOME"] = os.environ["APPDATA"] = TMP  # nunca toca nos arquivos reais
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "app"))
import ajustes  # noqa: E402
import substituicoes  # noqa: E402

LIGADOS = {c: True for c in ajustes.COMANDOS}


def ap(texto, comandos=None, regras=()):
    return substituicoes.aplicar(texto, LIGADOS if comandos is None else comandos, list(regras))


class Comandos(unittest.TestCase):
    def test_nova_linha_com_pontuacao_do_whisper(self):
        self.assertEqual(ap("Olá, nova linha. Tudo bem?"), "Olá\nTudo bem?")
        self.assertEqual(ap("Eu fiz isso. Nova linha. Depois fui."), "Eu fiz isso.\nDepois fui.")

    def test_ponto_e_virgula_sem_espaco_antes_e_maiuscula_depois(self):
        self.assertEqual(ap("Terminei ponto final, próxima frase"), "Terminei. Próxima frase")
        self.assertEqual(ap("Você vai Vírgula, né ponto de interrogação sim"), "Você vai, né? Sim")
        self.assertEqual(ap("Terminei. Ponto final."), "Terminei.")

    def test_parenteses(self):
        self.assertEqual(ap("Veja, abre parênteses, o anexo, fecha parênteses. Depois"), "Veja (o anexo). Depois")
        self.assertEqual(ap("veja abrir parêntese x fechar parêntese"), "veja (x)")

    def test_palavra_inteira_e_sem_comando_nao_mexe(self):
        self.assertEqual(ap("Os pontos finais e a nova linhagem"), "Os pontos finais e a nova linhagem")
        self.assertEqual(ap("texto  com  espaços. Nada."), "texto  com  espaços. Nada.")

    def test_comando_desligado_fica_como_dito(self):
        off = dict(LIGADOS, nova_linha=False)
        self.assertEqual(ap("uma nova linha de produtos", off), "uma nova linha de produtos")
        self.assertEqual(ap("oi vírgula tudo", off), "oi, tudo")  # os outros seguem ligados
        self.assertEqual(ap("oi vírgula tudo", {c: False for c in ajustes.COMANDOS}), "oi vírgula tudo")

    def test_comandos_seguidos(self):
        self.assertEqual(ap("a ponto final nova linha b"), "a.\nB")


class Regras(unittest.TestCase):
    def test_regra_propria_sem_diferenciar_maiusculas(self):
        self.assertEqual(ap("Meu E-mail é esse", regras=[("meu e-mail", "renan@betinhos.com.br")]),
                         "renan@betinhos.com.br é esse")

    def test_regra_propria_pontuacao_do_texto_fica(self):
        self.assertEqual(ap("Segue o meu assinatura.", regras=[("meu assinatura", "Att,\nRenan")]),
                         "Segue o Att,\nRenan.")

    def test_regra_so_em_palavra_inteira(self):
        self.assertEqual(ap("bolsa bol", regras=[("bol", "X")]), "bolsa X")

    def test_ler_arquivo(self):
        arq = os.path.join(tempfile.mkdtemp(), "s.txt")
        with open(arq, "w", encoding="utf-8") as f:
            f.write("# comentário\nbom dia => Bom dia, tudo bem?\n\nlinha => a\\nb\nsem seta\n => vazio\nx =>\n")
        self.assertEqual(substituicoes.ler_regras(arq), [("bom dia", "Bom dia, tudo bem?"), ("linha", "a\nb")])
        self.assertEqual(substituicoes.ler_regras(os.path.join(TMP, "nao-existe.txt")), [])

    def test_regra_vale_antes_do_comando(self):
        self.assertEqual(ap("ponto final", regras=[("ponto final", "FIM")]), "FIM")


class AjusteComandos(unittest.TestCase):
    def test_padrao_todos_ligados(self):
        self.assertEqual(ajustes.normalizar({})["comandos"], LIGADOS)

    def test_invalido_volta_a_ligado_e_valido_fica(self):
        a = ajustes.normalizar({"comandos": {"nova_linha": False, "virgula": "não", "extra": 1}})["comandos"]
        self.assertEqual((a["nova_linha"], a["virgula"], a["ponto_final"], a["extra"]), (False, True, True, 1))
        self.assertEqual(ajustes.normalizar({"comandos": "x"})["comandos"], LIGADOS)


if __name__ == "__main__":
    unittest.main()
