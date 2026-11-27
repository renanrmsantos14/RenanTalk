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
os.environ["XDG_CONFIG_HOME"] = os.environ["APPDATA"] = TMP  # nunca toca nos ajustes reais (Linux e Windows)
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "app"))
import comum  # noqa: E402
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
        with open(comum.ARQ_LOG, encoding="utf-8") as f:
            self.assertIn("ilegível", f.read())

    def test_chave_desconhecida_preservada(self):
        gravar(json.dumps({"volume": 10, "modos": {"app": "formal"}}))
        ajustes.salvar({"volume": 30})
        with open(ajustes.ARQ_AJUSTES, encoding="utf-8") as f:
            dados = json.load(f)
        self.assertEqual(dados["modos"], {"app": "formal"})
        self.assertEqual(dados["volume"], 30)
        self.assertFalse(os.path.exists(ajustes.ARQ_AJUSTES + ".tmp"))

    def test_atalho_invalido_vira_padrao(self):
        gravar(json.dumps({"atalho_ditar": "ctrl"}))
        self.assertEqual(ajustes.ler()["atalho_ditar"], "ctrl+win")

    def test_atalho_colar_padrao_invalido_e_igual(self):
        self.assertEqual(ajustes.ler()["atalho_colar"], "alt+win+v")
        for ruim in ("v", "alt+win+v+x", 5, ["alt", "win"], "ctrl+win", "ctrl+win+h"):  # inválidos, igual ou contendo o de ditar (ctrl+win)
            gravar(json.dumps({"atalho_colar": ruim}))
            ajustes._cache["mtime"] = -1
            self.assertEqual(ajustes.ler()["atalho_colar"], "alt+win+v", ruim)
        with open(comum.ARQ_LOG, encoding="utf-8") as f:
            self.assertIn("atalho_colar inválido ou em conflito", f.read())
        gravar(json.dumps({"atalho_colar": "Super+Alt+Shift+H"}))
        ajustes._cache["mtime"] = -1
        self.assertEqual(ajustes.ler()["atalho_colar"], "alt+shift+win+h")

    def test_luz_tema_notificacoes_padroes(self):
        a = ajustes.ler()
        self.assertEqual(a["luz"], {"ligada": True, "cor": "azul", "espessura": "media", "posicao": "embaixo"})
        self.assertEqual((a["tema"], a["notificacoes"]), ("sistema", True))

    def test_luz_tema_notificacoes_validos_e_invalidos(self):
        gravar(json.dumps({"luz": {"ligada": False, "cor": "roxo", "espessura": "grossa", "posicao": "topo"},
                           "tema": "escuro", "notificacoes": False}))
        a = ajustes.ler()
        self.assertEqual(a["luz"], {"ligada": False, "cor": "roxo", "espessura": "grossa", "posicao": "topo"})
        self.assertEqual((a["tema"], a["notificacoes"]), ("escuro", False))
        # inválidos voltam ao padrão, campo a campo (o que estava certo fica)
        gravar(json.dumps({"luz": {"ligada": "sim", "cor": "rosa", "espessura": "grossa", "posicao": 3, "extra": 1},
                           "tema": "azul", "notificacoes": "não"}))
        a = ajustes.ler()
        self.assertEqual(a["luz"], {"ligada": True, "cor": "azul", "espessura": "grossa", "posicao": "embaixo", "extra": 1})
        self.assertEqual((a["tema"], a["notificacoes"]), ("sistema", True))
        gravar(json.dumps({"luz": "verde"}))  # tipo errado inteiro
        self.assertEqual(ajustes.ler()["luz"], ajustes.PADROES["luz"])

    def test_padroes_da_luz_nao_sao_alterados_por_quem_le(self):
        ajustes.ler()["luz"]["cor"] = "verde"  # mutação do chamador não pode vazar para os padrões
        self.assertEqual(ajustes.PADROES["luz"]["cor"], "azul")

    def test_validar_atalho(self):
        v = ajustes.validar_atalho
        self.assertEqual(v("Super+Ctrl"), ("ctrl", "win"))
        self.assertEqual(v("ctrl+alt+d"), ("ctrl", "alt", "d"))
        for ruim in ("ctrl", "ctrl+ctrl", "d+e", "ctrl+ç", "", None, "d+f1",
                     "shift+a", "shift+space", "ctrl+v", "V+Ctrl", "ctrl+c", "alt+f4", "ctrl+f01"):
            self.assertIsNone(v(ruim), ruim)

    def test_atalho_contido_no_outro_conflita(self):
        self.assertTrue(ajustes.atalhos_conflitam("ctrl+win", "ctrl+win+v"))
        self.assertTrue(ajustes.atalhos_conflitam("ctrl+win+v", "ctrl+win"))
        self.assertFalse(ajustes.atalhos_conflitam("ctrl+win", "alt+win+v"))
        self.assertEqual(ajustes.normalizar({"atalho_colar": "ctrl+win+v"})["atalho_colar"], "alt+win+v")


    def test_abaixar_outros_padrao_validos_e_invalidos(self):
        a = ajustes.ler()
        self.assertEqual((a["abaixar_outros"], a["volume_outros"]), (False, 30))
        gravar(json.dumps({"abaixar_outros": True, "volume_outros": 150}))
        a = ajustes.ler()
        self.assertEqual((a["abaixar_outros"], a["volume_outros"]), (True, 100))
        gravar('{"abaixar_outros": "sim", "volume_outros": NaN}')
        a = ajustes.ler()
        self.assertEqual((a["abaixar_outros"], a["volume_outros"]), (False, 30))


class Som(unittest.TestCase):
    def test_lista_so_microfones_que_abrem_a_16k(self):
        sd = mock.MagicMock()
        sd.query_devices.side_effect = lambda kind=None: {"name": "Mic USB"} if kind else [
            {"index": 0, "name": "Mic USB", "max_input_channels": 1},
            {"index": 1, "name": "Mic WDM-KS", "max_input_channels": 2},
            {"index": 2, "name": "Alto-falante", "max_input_channels": 0},
        ]
        sd.check_input_settings.side_effect = lambda device, **_: None if device == 0 else (_ for _ in ()).throw(ValueError())
        with mock.patch.dict(sys.modules, {"sounddevice": sd}):
            self.assertEqual(ajustes.listar_entradas(), [{"nome": "Mic USB", "padrao": True}])

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


class AtalhoColar(unittest.TestCase):
    DITAR, COLAR = ("ctrl", "win"), ("alt", "win", "v")

    def passos(self, passos, gravando=False):
        c, saidas = ajustes.Casador(), []
        for nome, desceu in passos:
            saidas.append(c.evento(nome, desceu, self.DITAR, gravando, self.COLAR))
        return saidas

    def test_colar_e_ditar_separados(self):
        self.assertEqual(self.passos([("alt", True), ("win", True), ("v", True), ("v", False)]), [None, None, "colar", None])
        self.assertEqual(self.passos([("ctrl", True), ("win", True), ("win", False)]), [None, "iniciar", None])

    def test_colar_nao_repete_com_a_tecla_segurada(self):
        s = self.passos([("alt", True), ("win", True), ("v", True), ("v", True), ("v", False), ("v", True)])
        self.assertEqual(s, [None, None, "colar", None, None, "colar"])

    def test_tecla_do_colar_sozinha_ou_com_modificadores_errados_nao_dispara(self):
        self.assertEqual(self.passos([("v", True), ("v", False)]), [None, None])
        self.assertEqual(self.passos([("ctrl", True), ("alt", True), ("v", True)]), [None, None, None])

    def test_outra_tecla_continua_cancelando_o_ditado(self):
        c, g = ajustes.Casador(), False
        saidas = []
        for nome, d in [("ctrl", True), ("win", True), ("v", True), ("win", False)]:  # v é do colar, mas está gravando
            r = c.evento(nome, d, self.DITAR, g, self.COLAR)
            g = True if r == "iniciar" else (False if r == "parar" else g)
            saidas.append(r)
        self.assertEqual(saidas, [None, "iniciar", "outra", "parar"])

    def test_colar_ignorado_gravando_e_igual_ao_de_ditar(self):
        self.assertNotIn("colar", self.passos([("alt", True), ("win", True), ("v", True)], gravando=True))
        c = ajustes.Casador()
        self.assertEqual([c.evento(n, True, self.DITAR, False, self.DITAR) for n in ("ctrl", "win")], [None, "iniciar"])

    def test_esperar_soltar(self):
        c = ajustes.Casador()
        c.evento("alt", True, self.DITAR, False, self.COLAR)
        c.evento("alt", False, self.DITAR, False, self.COLAR)
        c.esperar_soltar(0.2)  # já solto: volta na hora
        c.apertadas.add("win")
        import time
        t0 = time.time()
        c.esperar_soltar(0.1)
        self.assertGreaterEqual(time.time() - t0, 0.09)


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

    def colar_ultimo(self, itens, gravando=False):
        d = self.d
        d.gravando = gravando
        feito = d.threading.Event()
        com_fim = lambda *a, **k: feito.set()
        with mock.patch.object(d.historico, "listar", return_value=itens), \
                mock.patch.object(d.plat, "colar", side_effect=com_fim) as colar, \
                mock.patch.object(d.plat, "bipe", side_effect=com_fim) as bipe:
            d.colar_ultimo(0.01)
            feito.wait(1)
            d.time.sleep(0.05)
        d.gravando = False
        return colar, bipe

    def test_colar_ultimo_cola_o_texto_mais_recente(self):
        colar, bipe = self.colar_ultimo([{"texto": "oi, tudo bem?"}])
        colar.assert_called_once_with("oi, tudo bem? ")
        bipe.assert_not_called()

    def test_abaixar_volume_so_com_o_ajuste_ligado(self):
        d = self.d
        for ligado, chamadas in ((False, 0), (True, 1)):
            with mock.patch.object(d.ajustes, "ler", return_value={"abaixar_outros": ligado, "volume_outros": 25}), \
                    mock.patch.object(d.plat, "abaixar_outros", return_value=[]) as abaixar:
                d.abaixar_volume()
            self.assertEqual(abaixar.call_count, chamadas)
        abaixar.assert_called_once_with(0.25)

    @unittest.skipIf(sys.platform == "win32", "usa o pactl do Linux")
    def test_abaixar_e_restaurar_outros_no_linux(self):
        plat = self.d.plat
        volumes = {36: [65536, 65536], 40: [16384]}

        def pactl(cmd, **_):
            if cmd[1] == "set-sink-input-volume":
                if int(cmd[2]) not in volumes:
                    raise plat.subprocess.CalledProcessError(1, cmd)
                volumes[int(cmd[2])] = [int(v) for v in cmd[3:]]
                return mock.Mock()
            lista = [{"index": i, "properties": {}, "volume": {f"c{n}": {"value": x} for n, x in enumerate(v)}}
                     for i, v in volumes.items()]
            return mock.Mock(stdout=json.dumps(lista).encode())

        with mock.patch.object(plat.subprocess, "run", side_effect=pactl):
            feitos = plat.abaixar_outros(0.3)
            self.assertEqual(volumes, {36: [19661, 19661], 40: [4915]})
            volumes[40] = [25600]  # o Spotify reage e muda o próprio volume no meio do ditado
            volumes[50] = volumes.pop(36)  # e outro app fechou e abriu
            plat.restaurar_outros(feitos)
        self.assertEqual(volumes, {50: [19661, 19661], 40: [16384]})

    @unittest.skipIf(sys.platform == "win32", "usa o pactl do Linux")
    def test_abaixar_sem_pactl_nao_impede_o_ditado(self):
        plat = self.d.plat
        with mock.patch.object(plat.subprocess, "run", side_effect=FileNotFoundError("pactl")):
            self.assertEqual(plat.abaixar_outros(0.3), [])
            plat.restaurar_outros([(36, None, [1])])
        self.assertEqual(plat._pendentes, {})

    @unittest.skipIf(sys.platform == "win32", "usa o pactl do Linux")
    def test_app_que_fecha_no_meio_volta_ao_volume_de_antes(self):
        """O PipeWire guarda o volume por app: sem isso, o app reaberto voltaria a 30%."""
        plat = self.d.plat
        fluxos = {36: ("Spotify", [65536]), 40: ("Firefox", [32768])}

        def pactl(cmd, **_):
            if cmd[1] == "set-sink-input-volume":
                if int(cmd[2]) not in fluxos:
                    raise plat.subprocess.CalledProcessError(1, cmd)
                fluxos[int(cmd[2])] = (fluxos[int(cmd[2])][0], [int(v) for v in cmd[3:]])
                return mock.Mock()
            lista = [{"index": i, "properties": {"application.name": a}, "volume": {"mono": {"value": v[0]}}}
                     for i, (a, v) in fluxos.items()]
            return mock.Mock(stdout=json.dumps(lista).encode())

        self.addCleanup(plat._pendentes.clear)
        with mock.patch.object(plat.subprocess, "run", side_effect=pactl):
            feitos = plat.abaixar_outros(0.5)
            fluxos[50] = ("Spotify", fluxos.pop(36)[1])  # o Spotify trocou de fluxo, ainda baixo
            del fluxos[40]  # o Firefox fechou
            plat.restaurar_outros(feitos)
            self.assertEqual(fluxos, {50: ("Spotify", [65536])})
            self.assertEqual(plat._pendentes, {"Firefox": [32768]})
            fluxos[60] = ("Firefox", [16384])  # reabriu com o volume baixo que o PipeWire guardou
            plat.restaurar_outros(plat.abaixar_outros(0.5))
        self.assertEqual(fluxos[60], ("Firefox", [32768]))
        self.assertEqual(plat._pendentes, {})

    @unittest.skipIf(sys.platform == "win32", "usa o pactl do Linux")
    def test_fluxo_fechado_nao_muda_outro_fluxo_do_mesmo_app(self):
        plat = self.d.plat
        fluxos = {36: [65536], 37: [32768]}  # dois fluxos "Chromium" com volumes diferentes

        def pactl(cmd, **_):
            if cmd[1] == "set-sink-input-volume":
                if int(cmd[2]) not in fluxos:
                    raise plat.subprocess.CalledProcessError(1, cmd)
                fluxos[int(cmd[2])] = [int(v) for v in cmd[3:]]
                return mock.Mock()
            lista = [{"index": i, "properties": {"application.name": "Chromium"}, "volume": {"mono": {"value": v[0]}}}
                     for i, v in fluxos.items()]
            return mock.Mock(stdout=json.dumps(lista).encode())

        self.addCleanup(plat._pendentes.clear)
        with mock.patch.object(plat.subprocess, "run", side_effect=pactl):
            feitos = plat.abaixar_outros(0.5)
            del fluxos[36]  # um fechou, o outro continua
            plat.restaurar_outros(feitos)
        self.assertEqual(fluxos, {37: [32768]})
        self.assertEqual(plat._pendentes, {"Chromium": [65536]})

    def test_colar_ultimo_sem_historico_faz_bipe_grave_e_nao_cola(self):
        colar, bipe = self.colar_ultimo([])
        colar.assert_not_called()
        bipe.assert_called_once_with(300, 150)

    def test_colar_ultimo_ignorado_gravando(self):
        colar, bipe = self.colar_ultimo([{"texto": "x"}], gravando=True)
        colar.assert_not_called()

    @unittest.skipIf(sys.platform == "win32", "usa a bandeja do Linux (notify-send)")
    def test_notificar_desligado_nao_chama_o_sistema(self):
        d = self.d
        bandeja = d.plat.Bandeja.__new__(d.plat.Bandeja)
        for ligada, chamadas in ((False, 0), (True, 1)):
            with mock.patch.object(d.ajustes, "ler", return_value={"notificacoes": ligada}), \
                    mock.patch.object(d.plat.subprocess, "Popen") as popen:
                bandeja.notificar("oi")
            self.assertEqual(popen.call_count, chamadas)

    @unittest.skipIf(sys.platform == "win32", "usa a luz do Linux (GTK)")
    def test_luz_le_ajustes_a_cada_ditado(self):
        luz = self.d.LUZ
        cfg = {"ligada": True, "cor": "verde", "espessura": "grossa", "posicao": "topo"}
        with mock.patch.object(self.d.ajustes, "ler", return_value={"luz": cfg}):
            luz._configurar()
        self.assertEqual((luz.topo, luz.altura, luz.k), (True, 72, 2.0))
        self.assertEqual(luz.cores["centro"], tuple(c / 255 for c in ajustes.CORES_LUZ["verde"]["centro"]))
        with mock.patch.object(self.d.ajustes, "ler", return_value={"luz": dict(cfg, ligada=False)}), \
                mock.patch.object(luz, "jan") as jan:
            luz._definir("rec")
            luz._definir("proc")
        jan.show.assert_not_called()
        self.assertIsNone(luz.timer)
        with mock.patch.object(self.d.ajustes, "ler", return_value={"luz": ajustes.PADROES["luz"]}):
            luz._configurar()  # devolve ao padrão para os outros testes
        self.assertEqual((luz.topo, luz.altura), (False, 36))

    def test_microfone_inexistente_abre_padrao_e_loga(self):
        d = self.d
        d.fluxo, d.fluxo_mic = None, None
        with mock.patch.object(d.ajustes, "ler", return_value={"microfone": "Microfone que não existe"}):
            d.garantir_fluxo()
        self.assertIsNotNone(d.fluxo)
        d.fluxo.close()
        with open(comum.ARQ_LOG, encoding="utf-8") as f:
            self.assertIn("não encontrado, usando o padrão", f.read())


if __name__ == "__main__":
    unittest.main()
