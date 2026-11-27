"""Testes da IA no texto (modos, fallback) e do atalho de traduzir. Rodar: python3 -m unittest discover -s tests"""
import importlib.machinery
import importlib.util
import io
import json
import os
import sys
import tempfile
import time
import unittest
from unittest import mock

TMP = tempfile.mkdtemp()
os.environ["XDG_CONFIG_HOME"] = os.environ["APPDATA"] = TMP  # nunca toca nos arquivos reais
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "app"))
import ajustes  # noqa: E402
import escrita  # noqa: E402


class Modo(unittest.TestCase):
    def test_app_escolhe_o_modo_e_o_resto_usa_o_padrao(self):
        a = ajustes.normalizar({"modo": "revisado"})
        self.assertEqual(escrita.modo_para("OUTLOOK", a), "formal")
        self.assertEqual(escrita.modo_para("WhatsApp.Root", a), "whatsapp")
        self.assertEqual(escrita.modo_para("code", a), "literal")
        self.assertEqual(escrita.modo_para("firefox", a), "revisado")
        self.assertEqual(escrita.modo_para("", a), "revisado")
        self.assertEqual(escrita.modo_para("olk", a), "formal")  # Outlook novo
        b = ajustes.normalizar({"modos_apps": {"code": "literal", "vscode": "revisado"}})
        self.assertEqual(escrita.modo_para("vscode", b), "revisado")  # o nome mais específico vale

    def test_ajustes_invalidos_voltam_ao_padrao(self):
        a = ajustes.normalizar({"modo": "poesia", "modos_apps": {" Slack ": "whatsapp", "x": "poesia", "": "formal"}})
        self.assertEqual(a["modo"], "literal")
        self.assertEqual(a["modos_apps"], {"slack": "whatsapp"})
        self.assertEqual(ajustes.normalizar({"modos_apps": []})["modos_apps"], {})


def resposta(conteudo):
    return io.BytesIO(json.dumps({"choices": [{"message": {"content": conteudo}}]}).encode())


class Aplicar(unittest.TestCase):
    def test_literal_nao_chama_a_ia(self):
        with mock.patch("urllib.request.urlopen") as u:
            self.assertEqual(escrita.aplicar("oi", "literal"), ("oi", "literal"))
        u.assert_not_called()

    def test_reescreve_com_o_modo_e_tira_as_marcacoes(self):
        with mock.patch("urllib.request.urlopen", return_value=resposta(" <ditado>Olá, às 10\u202fh?</ditado> ")) as u:
            self.assertEqual(escrita.aplicar("ola tudo bem", "revisado"), ("Olá, às 10 h?", "revisado"))
        corpo = json.loads(u.call_args.args[0].data)
        self.assertEqual(corpo["model"], escrita.MODELO)
        self.assertIn("<ditado>ola tudo bem</ditado>", corpo["messages"][1]["content"])
        self.assertIn("Nunca responda", corpo["messages"][0]["content"])

    def test_falha_ou_resposta_vazia_cola_o_literal(self):
        for efeito in (OSError("sem rede"), None):
            with self.subTest(efeito=efeito), mock.patch("urllib.request.urlopen", side_effect=efeito,
                                                         return_value=resposta("  ")), \
                    mock.patch.object(escrita, "log") as log:
                self.assertEqual(escrita.aplicar("texto cru", "formal"), ("texto cru", "literal"))
                self.assertNotIn("texto cru", log.call_args.args[0])  # o log nunca leva o conteúdo

    def test_demorou_mais_que_o_limite_cola_o_literal(self):
        def lenta(*_a, **_k):
            time.sleep(0.5)
            return resposta("tarde demais")

        with mock.patch.object(escrita, "LIMITE_S", 0.1), mock.patch("urllib.request.urlopen", side_effect=lenta):
            inicio = time.time()
            self.assertEqual(escrita.aplicar("rápido", "whatsapp"), ("rápido", "literal"))
            self.assertLess(time.time() - inicio, 0.4)


class AtalhoTraduzir(unittest.TestCase):
    DITAR, COLAR, TRADUZIR = ("ctrl", "win"), ("alt", "win", "v"), ("shift", "win")

    def ev(self, c, nome, descer, gravando):
        return c.evento(nome, descer, self.DITAR, gravando, self.COLAR, self.TRADUZIR)

    def test_segurar_traduz_e_soltar_para(self):
        c = ajustes.Casador()
        self.assertIsNone(self.ev(c, "shift", True, False))
        self.assertEqual(self.ev(c, "win", True, False), "traduzir")
        self.assertIsNone(self.ev(c, "shift", True, True))  # repetição da tecla segurada
        self.assertEqual(self.ev(c, "shift", False, True), "parar")

    def test_ditar_continua_igual_e_outra_tecla_cancela(self):
        c = ajustes.Casador()
        self.ev(c, "ctrl", True, False)
        self.assertEqual(self.ev(c, "win", True, False), "iniciar")
        self.assertEqual(self.ev(c, "shift", True, True), "outra")  # shift é do traduzir, não do ditar
        self.assertEqual(self.ev(c, "ctrl", False, True), "parar")

    def test_padrao_e_conflitos(self):
        self.assertEqual(ajustes.normalizar({})["atalho_traduzir"], "shift+win")
        self.assertEqual(ajustes.normalizar({"atalho_traduzir": "ctrl+win+t"})["atalho_traduzir"], "shift+win")
        # o de ditar ocupou o padrão do traduzir: traduzir fica desligado em vez de disparar no lugar do outro
        self.assertEqual(ajustes.normalizar({"atalho_ditar": "shift+win"})["atalho_traduzir"], "")


class Fluxo(unittest.TestCase):
    """transcrever_e_colar de ponta a ponta, com o Whisper, a área de transferência e a rede simulados."""

    @classmethod
    def setUpClass(cls):
        os.environ.setdefault("DISPLAY", ":0")
        caminho = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "app", "ditado.pyw")
        loader = importlib.machinery.SourceFileLoader("ditado", caminho)
        cls.d = importlib.util.module_from_spec(importlib.util.spec_from_loader("ditado", loader))
        loader.exec_module(cls.d)

    def rodar(self, app, em_ingles=False, ia=None):
        d = self.d
        with mock.patch.object(d, "transcrever", return_value="ola tudo bem"), \
                mock.patch.object(d.plat, "app_ativo", return_value=app), \
                mock.patch.object(d.plat, "colar") as colar, \
                mock.patch.object(d.plat, "bipe") as self.bipe, \
                mock.patch.object(d.historico, "gravar") as gravar, \
                mock.patch.object(d.ajustes, "ler", return_value=ajustes.normalizar({})), \
                mock.patch("urllib.request.urlopen", **ia) as u:
            d.transcrever_e_colar(b"", 1.0, em_ingles)
        return colar.call_args.args[0], gravar.call_args.kwargs["modo"], u

    def test_app_com_modo_reescreve_e_grava_o_modo(self):
        colado, modo, _ = self.rodar("OUTLOOK.EXE", ia={"return_value": resposta("Olá, tudo bem?")})
        self.assertEqual((colado, modo), ("Olá, tudo bem? ", "formal"))

    def test_padrao_literal_nao_chama_a_ia(self):
        colado, modo, u = self.rodar("firefox", ia={})
        self.assertEqual((colado, modo), ("ola tudo bem ", "literal"))
        u.assert_not_called()
        self.bipe.assert_not_called()

    def test_traduzir_ignora_o_modo_do_app(self):
        colado, modo, u = self.rodar("code", em_ingles=True, ia={"return_value": resposta("Hi, how are you?")})
        self.assertEqual((colado, modo), ("Hi, how are you? ", "ingles"))
        self.assertIn("inglês", json.loads(u.call_args.args[0].data)["messages"][0]["content"])

    def test_ia_fora_do_ar_cola_o_literal(self):
        colado, modo, _ = self.rodar("whatsapp", ia={"side_effect": OSError("sem rede")})
        self.assertEqual((colado, modo), ("ola tudo bem ", "literal"))
        self.bipe.assert_called_once_with(300, 150)  # a falha não passa em silêncio


class ConferirModelos(unittest.TestCase):
    def conferir(self, ids, chave="k"):
        import comum
        resposta = io.BytesIO(json.dumps({"data": [{"id": i, "active": True} for i in ids]}).encode())
        with mock.patch.object(comum, "ler_chave", return_value=chave), \
                mock.patch.object(comum.urllib.request, "urlopen", return_value=resposta) as rede, \
                mock.patch.object(comum, "log") as log:
            comum.conferir_modelos("whisper-large-v3", escrita.MODELO)
        return [c.args[0] for c in log.call_args_list], rede

    def test_modelo_desligado_vai_ao_log(self):
        linhas, _ = self.conferir(["whisper-large-v3"])
        self.assertEqual(len(linhas), 1)
        self.assertIn(escrita.MODELO, linhas[0])

    def test_todos_ativos_ou_sem_chave_nao_avisa(self):
        self.assertEqual(self.conferir(["whisper-large-v3", escrita.MODELO])[0], [])
        linhas, rede = self.conferir([], chave="")
        self.assertEqual(linhas, [])
        rede.assert_not_called()


if __name__ == "__main__":
    unittest.main()
