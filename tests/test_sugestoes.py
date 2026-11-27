"""Testes das sugestões de termos por frequência. Rodar: python3 -m unittest discover -s tests"""
import os
import sys
import tempfile
import unittest
from datetime import datetime, timedelta
from unittest import mock

TMP = tempfile.mkdtemp()
os.environ["XDG_CONFIG_HOME"] = os.environ["APPDATA"] = TMP  # nunca toca no histórico real
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "app"))
import historico  # noqa: E402


class Sugestoes(unittest.TestCase):
    def setUp(self):
        self.arq = os.path.join(tempfile.mkdtemp(), "h.db")
        self.vocab = []
        p = mock.patch.object(historico, "ler_vocabulario", lambda: self.vocab)
        p.start()
        self.addCleanup(p.stop)

    def g(self, texto, n=1):
        for _ in range(n):
            historico.gravar(texto, arquivo=self.arq)

    def sug(self):
        con = historico.conectar(self.arq)
        try:
            return {l["termo"]: (l["motivo"], l["contagem"], l["estado"]) for l in con.execute("SELECT * FROM sugestao")}
        finally:
            con.close()

    def sql(self, comando, *args):
        con = historico.conectar(self.arq)
        try:
            con.execute(comando, args)
            con.commit()
        finally:
            con.close()

    def test_cinco_vira_sugestao_quatro_nao(self):
        self.g("falei com Marcondes sobre o prazo", 4)
        self.assertEqual(self.sug(), {})
        self.g("falei com Marcondes sobre o prazo")
        self.assertEqual(self.sug(), {"Marcondes": ("frequencia", 5, "nova")})

    def test_inicio_de_frase_nao_conta_mas_sigla_sim(self):
        self.g("Marcondes ligou. Marcondes pediu. Hoje o CTF subiu", 5)
        self.assertEqual(self.sug(), {"CTF": ("frequencia", 5, "nova")})

    def test_nome_composto_vira_um_termo_so(self):
        self.vocab = ["Betinhos Executive Service"]
        self.g("buscar no Santos Dumont, Galeão Rio e a Betinhos Executive", 5)
        self.assertEqual(set(self.sug()), {"Santos Dumont", "Galeão Rio"})

    def test_vocabulario_e_palavras_comuns_ficam_de_fora(self):
        self.vocab = ["renantalk", "Betinhos Executive Service"]
        self.g("vi o RenanTalk e o Executive e Você ou Não e Sim", 5)
        self.assertEqual(self.sug(), {})

    def test_contagem_atualiza_e_ignorada_nao_volta(self):
        self.g("o Marcondes ligou", 5)
        self.sql("UPDATE sugestao SET estado='ignorada'")
        self.g("o marcondes e o Marcondes ligaram")
        self.assertEqual(self.sug(), {"Marcondes": ("frequencia", 6, "ignorada")})
        self.sql("UPDATE sugestao SET estado='aceita'")
        self.g("de novo o Marcondes")
        self.assertEqual(self.sug()["Marcondes"][2:], ("aceita",))

    def test_so_ultimos_30_dias(self):
        self.g("o Marcondes ligou", 5)
        velho = (datetime.now() - timedelta(days=31)).strftime("%Y-%m-%dT%H:%M:%S")
        self.sql("UPDATE ditado SET criado_em=?", velho)
        self.sql("DELETE FROM sugestao")
        self.g("o Marcondes ligou", 2)
        self.assertEqual(self.sug(), {})

    def test_falha_nas_sugestoes_nao_impede_gravar(self):
        with mock.patch.object(historico.sugestoes, "atualizar", side_effect=RuntimeError("x")):
            self.assertIsNotNone(historico.gravar("oi", arquivo=self.arq))
        self.assertEqual(len(historico.listar(arquivo=self.arq)), 1)


if __name__ == "__main__":
    unittest.main()
