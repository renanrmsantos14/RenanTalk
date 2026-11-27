"""Testes de ajustes.json, bipe, idioma, microfone e atalho. Rodar: python3 -m unittest discover -s tests"""
import importlib.machinery
import importlib.util
import io
import json
import os
import sys
import tempfile
import unittest
from unittest import mock

TMP = tempfile.mkdtemp()
os.environ["XDG_CONFIG_HOME"] = TMP  # nunca toca nos ajustes reais
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "app"))
import ajustes  # noqa: E402


def gravar(conteudo: str) -> None:
    with open(ajustes.ARQ_AJUSTES, "w", encoding="utf-8") as f:
        f.write(conteudo)
    os.utime(ajustes.ARQ_AJUSTES, ns=(0, __import__("time").time_ns()))


class Ajustes(unittest.TestCase):
    def setUp(self):
        if os.path.exists(ajustes.ARQ_AJUSTES):
            os.remove(ajustes.ARQ_AJUSTES)
        ajustes._cache["mtime"] = -1

    def test_ausente_usa_padroes(self):
        self.assertEqual(ajustes.ler(), ajustes.PADROES)

    def test_corrompido_usa_padroes_e_loga(self):
        gravar("{nao é json")
        self.assertEqual(ajustes.ler(), ajustes.PADROES)
        with open(ajustes.ARQ_LOG, encoding="utf-8") as f:
            self.assertIn("ilegível", f.read())

    def test_chave_desconhecida_preservada(self):
        gravar(json.dumps({"volume": 10, "luz": {"cor": "azul"}}))
        ajustes.salvar({"volume": 30})
        with open(ajustes.ARQ_AJUSTES, encoding="utf-8") as f:
            dados = json.load(f)
        self.assertEqual(dados["luz"], {"cor": "azul"})
        self.assertEqual(dados["volume"], 30)
        self.assertFalse(os.path.exists(ajustes.ARQ_AJUSTES + ".tmp"))

    def test_atalho_invalido_vira_padrao(self):
        gravar(json.dumps({"atalho_ditar": "ctrl"}))
        self.assertEqual(ajustes.ler()["atalho_ditar"], "ctrl+win")

    def test_validar_atalho(self):
        v = ajustes.validar_atalho
        self.assertEqual(v("Super+Ctrl"), ("ctrl", "win"))
        self.assertEqual(v("ctrl+alt+d"), ("ctrl", "alt", "d"))
        for ruim in ("ctrl", "ctrl+ctrl", "d+e", "ctrl+ç", "", None, "d+f1"):
            self.assertIsNone(v(ruim), ruim)


class Som(unittest.TestCase):
    def test_pico_proporcional_ao_volume(self):
        self.assertAlmostEqual(float(abs(ajustes.onda_bipe(880, 60, 30)).max()), 0.3, delta=0.01)
        self.assertEqual(float(abs(ajustes.onda_bipe(880, 60, 0)).max()), 0.0)

    def test_volume_zero_nao_toca(self):
        sd = mock.MagicMock()
        with mock.patch.dict(sys.modules, {"sounddevice": sd}), mock.patch.object(ajustes, "ler", return_value={"volume": 0}):
            ajustes.bipe(880, 60)
        sd.play.assert_not_called()
        with mock.patch.dict(sys.modules, {"sounddevice": sd}), mock.patch.object(ajustes, "ler", return_value={"volume": 30}):
            ajustes.bipe(880, 60)
        sd.play.assert_called_once()
        self.assertAlmostEqual(float(abs(sd.play.call_args[0][0]).max()), 0.3, delta=0.01)


class Atalho(unittest.TestCase):
    def rodar(self, alvo, passos, gravando_apos=("iniciar",)):
        c, gravando, saidas = ajustes.Casador(), False, []
        for nome, desceu in passos:
            r = c.evento(nome, desceu, alvo, gravando)
            if r == "iniciar":
                gravando = True
            elif r == "parar":
                gravando = False
            saidas.append(r)
        return saidas

    def test_ctrl_alt(self):
        s = self.rodar(("ctrl", "alt"), [("ctrl", True), ("alt", True), ("alt", False)])
        self.assertEqual(s, [None, "iniciar", "parar"])

    def test_ctrl_win_nao_dispara_com_alt(self):
        s = self.rodar(("ctrl", "win"), [("ctrl", True), ("alt", True), ("win", True)])
        self.assertEqual(s, [None, None, None])

    def test_outra_tecla_cancela(self):
        s = self.rodar(("ctrl", "win"), [("ctrl", True), ("win", True), ("x", True), ("win", False)])
        self.assertEqual(s, [None, "iniciar", "outra", "parar"])

    def test_modificadores_mais_tecla(self):
        s = self.rodar(("ctrl", "alt", "d"), [("ctrl", True), ("alt", True), ("d", True), ("d", False)])
        self.assertEqual(s, [None, None, "iniciar", "parar"])


class Ditado(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        os.environ.setdefault("DISPLAY", ":0")
        caminho = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "app", "ditado.pyw")
        loader = importlib.machinery.SourceFileLoader("ditado", caminho)
        spec = importlib.util.spec_from_loader("ditado", loader)
        cls.d = importlib.util.module_from_spec(spec)
        loader.exec_module(cls.d)

    def corpo(self, idioma):
        capturado = {}

        class Resp(io.BytesIO):
            def __enter__(self): return self
            def __exit__(self, *a): return False

        def falso(req, timeout=0):
            capturado["corpo"] = req.data
            return Resp(b'{"text": "oi"}')

        with mock.patch.object(self.d.ajustes, "ler", return_value={"idioma": idioma}), \
                mock.patch.object(self.d.urllib.request, "urlopen", falso):
            self.d.transcrever(b"\0\0")
        return capturado["corpo"]

    def test_idioma_no_multipart(self):
        self.assertNotIn(b'name="language"', self.corpo("auto"))
        self.assertRegex(self.corpo("en"), rb'name="language"\r\n\r\nen\r\n')

    def test_microfone_inexistente_abre_padrao_e_loga(self):
        d = self.d
        d.fluxo, d.fluxo_mic = None, None
        with mock.patch.object(d.ajustes, "ler", return_value={"microfone": "Microfone que não existe"}):
            d.garantir_fluxo()
        self.assertIsNotNone(d.fluxo)
        d.fluxo.close()
        with open(d.ARQ_LOG, encoding="utf-8") as f:
            self.assertIn("não encontrado, usando o padrão", f.read())


if __name__ == "__main__":
    unittest.main()
