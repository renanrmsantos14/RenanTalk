"""Testes do histórico de ditados. Rodar: python3 -m unittest discover -s tests"""
import os
import sqlite3
import sys
import tempfile
import unittest
from datetime import datetime, timedelta
from unittest import mock

TMP = tempfile.mkdtemp()
os.environ["XDG_CONFIG_HOME"] = TMP  # nunca toca no histórico real
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "app"))
import historico  # noqa: E402


class Historico(unittest.TestCase):
    def setUp(self):
        self.arq = os.path.join(tempfile.mkdtemp(), "h.db")

    def g(self, texto="olá mundo", **kw):
        return historico.gravar(texto, arquivo=self.arq, **kw)

    def test_banco_ausente_e_criado_com_schema(self):
        self.assertFalse(os.path.exists(self.arq))
        self.assertEqual(historico.listar(arquivo=self.arq), [])
        con = sqlite3.connect(self.arq)
        self.assertEqual(con.execute("PRAGMA user_version").fetchone()[0], historico.VERSAO)
        self.assertEqual(con.execute("PRAGMA journal_mode").fetchone()[0], "wal")
        self.assertEqual(con.execute("SELECT count(*) FROM sugestao").fetchone()[0], 0)

    def test_gravar_guarda_campos(self):
        i = self.g("Reunião às 15h 😀", app="code", idioma="pt", duracao_s=2.345)
        (l,) = historico.listar(arquivo=self.arq)
        self.assertEqual((l["id"], l["texto"], l["original"], l["app"], l["modo"], l["idioma"], l["palavras"], l["editado"]),
                         (i, "Reunião às 15h 😀", "Reunião às 15h 😀", "code", "literal", "pt", 4, 0))
        self.assertEqual(l["duracao_s"], 2.35)

    def test_busca_like_ignora_curingas_e_acentos_exatos(self):
        self.g("fechar a cotação do cliente")
        self.g("100% certo")
        self.g("outro texto")
        self.assertEqual(len(historico.listar("cotação", arquivo=self.arq)), 1)
        self.assertEqual(len(historico.listar("100%", arquivo=self.arq)), 1)
        self.assertEqual(len(historico.listar("%", arquivo=self.arq)), 1)  # % literal, não curinga
        self.assertEqual(len(historico.listar("", arquivo=self.arq)), 3)

    def test_editar_marca_editado_e_preserva_original(self):
        i = self.g("renan talk")
        self.assertTrue(historico.editar(i, "RenanTalk", arquivo=self.arq))
        (l,) = historico.listar(arquivo=self.arq)
        self.assertEqual((l["texto"], l["original"], l["editado"]), ("RenanTalk", "renan talk", 1))
        self.assertFalse(historico.editar(999, "x", arquivo=self.arq))

    def test_excluir_e_apagar_tudo(self):
        a, _ = self.g("um"), self.g("dois")
        self.assertTrue(historico.excluir(a, arquivo=self.arq))
        self.assertEqual([x["texto"] for x in historico.listar(arquivo=self.arq)], ["dois"])
        self.g("três")
        self.assertEqual(historico.apagar_tudo(arquivo=self.arq), 2)
        self.assertEqual(historico.listar(arquivo=self.arq), [])

    def test_retencao_apaga_so_mais_de_30_dias(self):
        self.g("recente")
        self.g("velho")
        con = historico.conectar(self.arq)
        antigo = (datetime.now() - timedelta(days=31)).strftime("%Y-%m-%dT%H:%M:%S")
        limite = (datetime.now() - timedelta(days=29)).strftime("%Y-%m-%dT%H:%M:%S")
        con.execute("UPDATE ditado SET criado_em=? WHERE texto='velho'", (antigo,))
        con.execute("INSERT INTO ditado(criado_em,texto,original) VALUES(?,?,?)", (limite, "29 dias", "29 dias"))
        con.commit(); con.close()
        self.assertEqual(historico.limpar_antigos(arquivo=self.arq), 1)
        self.assertEqual(sorted(x["texto"] for x in historico.listar(arquivo=self.arq)), ["29 dias", "recente"])

    def test_erro_de_gravacao_nao_propaga(self):
        with mock.patch.object(historico, "conectar", side_effect=sqlite3.OperationalError("disk I/O")):
            self.assertIsNone(historico.gravar("segredo-xyz"))
            self.assertEqual(historico.limpar_antigos(), 0)
        with open(historico.ARQ_LOG, encoding="utf-8") as f:
            log = f.read()
        self.assertIn("não gravou", log)
        self.assertNotIn("segredo-xyz", log)

    def test_banco_travado_desiste_rapido(self):
        self.g("existe")
        trava = sqlite3.connect(self.arq, isolation_level=None)
        trava.execute("BEGIN EXCLUSIVE")
        try:
            with mock.patch.object(historico, "TIMEOUT_S", 0.2):
                inicio = datetime.now()
                self.assertIsNone(self.g("bloqueado"))
                self.assertLess((datetime.now() - inicio).total_seconds(), 2)
        finally:
            trava.execute("ROLLBACK"); trava.close()

    def test_texto_longo(self):
        longo = "palavra " * 20000
        i = self.g(longo)
        self.assertEqual(historico.listar(arquivo=self.arq)[0]["palavras"], 20000)
        self.assertIsNotNone(i)


if __name__ == "__main__":
    unittest.main()
