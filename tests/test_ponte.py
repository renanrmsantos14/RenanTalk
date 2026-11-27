"""Testes da versão-ponte (app/ponte.py) e da assinatura minisign. Rodar: python3 -m unittest discover -s tests

Os dados de tests/dados/ponte-* foram assinados com uma chave de teste do `tauri signer` (só a pública fica aqui).
"""
import json
import os
import sys
import tempfile
import unittest
from unittest import mock

TMP = tempfile.mkdtemp(prefix="renantalk-teste-ponte-")
os.environ["XDG_CONFIG_HOME"] = os.environ["APPDATA"] = TMP  # nunca toca nos arquivos reais
RAIZ = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
sys.path.insert(0, os.path.join(RAIZ, "app"))
import minisign  # noqa: E402
import ponte  # noqa: E402

DADOS = os.path.join(RAIZ, "tests", "dados")


def _ler(nome: str, modo: str = "r"):
    with open(os.path.join(DADOS, nome), modo) as f:
        return f.read()


PACOTE = _ler("ponte-pacote.bin", "rb")
SIG = _ler("ponte-pacote.bin.sig").strip()
PUB = _ler("ponte-teste.pub").strip()


class Integridade(unittest.TestCase):
    def test_assinatura_boa(self):
        self.assertIn("ponte-pacote.bin", minisign.conferir(PACOTE, SIG, PUB))

    def test_pacote_adulterado(self):
        for ruim in (PACOTE + b"x", PACOTE[:-1], bytes([PACOTE[0] ^ 1]) + PACOTE[1:]):
            with self.assertRaises(minisign.AssinaturaInvalida):
                minisign.conferir(ruim, SIG, PUB)

    def test_outra_chave(self):
        with self.assertRaisesRegex(minisign.AssinaturaInvalida, "outra chave"):
            minisign.conferir(PACOTE, SIG, ponte.PUBKEY)

    def test_assinatura_lixo(self):
        for ruim in ("", "não é base64", "dW50cnVzdGVkIGNvbW1lbnQ6IHgK"):
            with self.assertRaises(minisign.AssinaturaInvalida):
                minisign.conferir(PACOTE, ruim, PUB)

    def test_comentario_confiavel_adulterado(self):
        import base64

        linhas = base64.b64decode(SIG).decode().split("\n")
        linhas[2] = linhas[2] + "x"
        with self.assertRaisesRegex(minisign.AssinaturaInvalida, "comentário"):
            minisign.conferir(PACOTE, base64.b64encode("\n".join(linhas).encode()).decode(), PUB)

    def test_rfc8032_vetor_1(self):
        chave = bytes.fromhex("d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a")
        sig = bytes.fromhex("e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555fb8821590a33bacc61e39701cf9b46"
                            "bd25bf5f0595bbe24655141438e7a100b")
        self.assertTrue(minisign.ed25519_confere(chave, b"", sig))
        self.assertFalse(minisign.ed25519_confere(chave, b"\x00", sig))

    def test_pubkey_igual_a_do_app(self):
        with open(os.path.join(RAIZ, "apps", "desktop", "src-tauri", "tauri.conf.json"), encoding="utf-8") as f:
            self.assertEqual(ponte.PUBKEY, json.load(f)["plugins"]["updater"]["pubkey"])
        minisign.ler_chave(ponte.PUBKEY)  # formato válido


class Contador(unittest.TestCase):
    def setUp(self):
        self.pasta = tempfile.mkdtemp(dir=TMP)

    def test_conta_e_avisa_na_terceira(self):
        self.assertEqual(ponte.falhas_seguidas(self.pasta), 0)
        ns = [ponte.registrar_falha(self.pasta) for _ in range(6)]
        self.assertEqual(ns, [1, 2, 3, 4, 5, 6])
        self.assertEqual([ponte.deve_avisar(n) for n in ns], [False, False, True, False, False, True])

    def test_arquivo_estragado_recomeca(self):
        with open(os.path.join(self.pasta, ponte.FALHAS), "w") as f:
            f.write("lixo")
        self.assertEqual(ponte.registrar_falha(self.pasta), 1)


class Confirmacao(unittest.TestCase):
    def setUp(self):
        self.pasta = tempfile.mkdtemp(dir=TMP)

    def marcar(self, conteudo):
        with open(os.path.join(self.pasta, ponte.MARCADOR), "w") as f:
            f.write(conteudo if isinstance(conteudo, str) else json.dumps(conteudo))

    def test_sem_marcador(self):
        self.assertFalse(ponte.confirmado("0.0.9", self.pasta))

    def test_so_com_versao_gancho_e_autostart(self):
        bom = {"versao": "0.0.9", "pid": 1, "gancho": True, "autostart": True}
        self.marcar(bom)
        self.assertTrue(ponte.confirmado("0.0.9", self.pasta))
        for ruim in ({**bom, "versao": "0.0.8"}, {**bom, "gancho": False}, {**bom, "autostart": False},
                     {**bom, "autostart": "true"}, [], "{quebrado"):
            self.marcar(ruim)
            self.assertFalse(ponte.confirmado("0.0.9", self.pasta), ruim)

    def test_espera_falha_sem_marcador(self):
        with self.assertRaises(ponte.Falha) as c:
            ponte.esperar_confirmacao("0.0.9", None, espera=0.6, pasta=self.pasta)
        self.assertEqual(c.exception.etapa, "confirmação")

    def test_app_que_fecha_sozinho(self):
        proc = mock.Mock(returncode=1)
        proc.poll.return_value = 1
        with self.assertRaisesRegex(ponte.Falha, "fechou sozinho"):
            ponte.esperar_confirmacao("0.0.9", proc, espera=5, pasta=self.pasta)


class Fluxo(unittest.TestCase):
    """A decisão de remover o Python: só com o download íntegro, a instalação e a confirmação."""

    def setUp(self):
        self.pasta = tempfile.mkdtemp(dir=TMP)
        self.alvo = {"versao": "0.0.9", "url": "http://x/RenanTalk_0.0.9_amd64.AppImage", "assinatura": SIG}
        proc = mock.Mock(pid=4321)
        proc.poll.return_value = None
        self.p = {n: mock.patch.object(ponte, n) for n in
                  ("_baixar", "instalar", "iniciar", "remover_python", "_encerrar_rust_existente", "_encerrar",
                   "desfazer_autostart_rust", "esperar_confirmacao")}
        self.m = {n: p.start() for n, p in self.p.items()}
        self.m["_baixar"].return_value = PACOTE
        self.m["iniciar"].return_value = proc
        for p in (mock.patch.object(ponte, "PASTA", self.pasta), mock.patch.dict(os.environ, RENANTALK_TESTE_PUBKEY=PUB),
                  mock.patch.object(ponte, "log")):
            p.start()
        self.addCleanup(mock.patch.stopall)
        self.avisos = []

    def rodar(self):
        return ponte.executar(self.alvo, notificar=self.avisos.append)

    def test_confirmado_remove_o_python(self):
        self.assertTrue(self.rodar())
        self.m["remover_python"].assert_called_once()
        self.m["desfazer_autostart_rust"].assert_not_called()
        self.assertEqual(ponte.falhas_seguidas(self.pasta), 0)

    def test_sem_confirmacao_nao_remove(self):
        self.m["esperar_confirmacao"].side_effect = ponte.Falha("confirmação", "sem marcador")
        self.assertFalse(self.rodar())
        self.m["remover_python"].assert_not_called()
        self.m["_encerrar"].assert_called_once_with(self.m["iniciar"].return_value)  # o app novo que subiu sai
        self.m["desfazer_autostart_rust"].assert_called_once()
        self.assertEqual(ponte.falhas_seguidas(self.pasta), 1)

    def test_pacote_adulterado_nem_instala(self):
        self.m["_baixar"].return_value = PACOTE + b"x"
        self.assertFalse(self.rodar())
        self.m["instalar"].assert_not_called()
        self.m["iniciar"].assert_not_called()
        self.m["remover_python"].assert_not_called()

    def test_chave_de_producao_recusa_pacote_de_teste(self):
        with mock.patch.dict(os.environ, RENANTALK_TESTE_PUBKEY=""):
            self.assertFalse(self.rodar())
        self.m["instalar"].assert_not_called()

    def test_download_e_instalacao_falhando(self):
        self.m["_baixar"].side_effect = OSError("sem rede")
        self.assertFalse(self.rodar())
        self.m["_baixar"].side_effect = None
        self.m["instalar"].side_effect = ponte.Falha("instalação", "NSIS saiu com 2")
        self.assertFalse(self.rodar())
        self.m["iniciar"].assert_not_called()
        self.m["remover_python"].assert_not_called()
        self.assertEqual(ponte.falhas_seguidas(self.pasta), 2)

    def test_avisa_na_terceira_falha_seguida(self):
        self.m["esperar_confirmacao"].side_effect = ponte.Falha("confirmação", "sem marcador")
        for _ in range(3):
            self.rodar()
        self.assertEqual(len(self.avisos), 1)
        self.assertIn("3 tentativas", self.avisos[0])
        self.m["remover_python"].assert_not_called()


class Remocao(unittest.TestCase):
    def setUp(self):
        base = tempfile.mkdtemp(dir=TMP)
        self.dados = os.path.join(base, "config", "RenanTalk")
        self.rust = os.path.join(base, "local", "RenanTalk")
        self.app = os.path.join(self.rust, "app")
        for d in (self.dados, self.app):
            os.makedirs(d)
        for arq in (os.path.join(self.app, "ditado.pyw"), os.path.join(self.dados, "chave.txt"),
                    os.path.join(self.rust, "renantalk.exe")):
            open(arq, "w").close()
        for p in (mock.patch.object(ponte, "PASTA", self.dados), mock.patch.object(ponte, "DIR_RUST", self.rust),
                  mock.patch.object(ponte, "APP_INSTALADOR", self.app), mock.patch.object(ponte, "log"),
                  mock.patch.object(ponte, "WINDOWS", False)):
            p.start()
        self.autostart = mock.patch.object(ponte, "definir_inicio_automatico").start()
        self.addCleanup(mock.patch.stopall)

    def test_remove_so_a_pasta_do_app(self):
        ponte.remover_python(self.app)
        self.autostart.assert_called_once_with(False)
        self.assertFalse(os.path.exists(self.app))
        self.assertTrue(os.path.exists(os.path.join(self.dados, "chave.txt")))
        self.assertTrue(os.path.exists(os.path.join(self.rust, "renantalk.exe")))

    def test_python_fora_do_instalador_fica(self):
        clone = os.path.join(TMP, "clone", "app")
        os.makedirs(clone)
        open(os.path.join(clone, "ditado.pyw"), "w").close()
        ponte.remover_python(clone)
        self.assertTrue(os.path.exists(clone))
        self.assertTrue(os.path.exists(self.app))

    def test_pasta_sem_ditado_fica(self):
        os.remove(os.path.join(self.app, "ditado.pyw"))
        ponte.remover_python(self.app)
        self.assertTrue(os.path.exists(self.app))


class Procurar(unittest.TestCase):
    def test_release_sem_latest_json(self):
        self.assertIsNone(ponte.procurar({"assets": [{"name": "RenanTalk.zip"}]}))

    def test_latest_json_da_plataforma(self):
        info = {"version": "0.0.9", "platforms": {ponte.PLATAFORMA: {"url": "http://x/p", "signature": "abc"}}}
        with mock.patch.object(ponte, "_baixar", return_value=json.dumps(info).encode()):
            alvo = ponte.procurar({"assets": [{"name": "latest.json", "browser_download_url": "http://x/l"}]})
        self.assertEqual(alvo, {"versao": "0.0.9", "url": "http://x/p", "assinatura": "abc"})

    def test_latest_json_sem_a_plataforma(self):
        info = {"version": "0.0.9", "platforms": {"darwin-aarch64": {"url": "u", "signature": "s"}}}
        with mock.patch.object(ponte, "_baixar", return_value=json.dumps(info).encode()):
            self.assertIsNone(ponte.procurar({"assets": [{"name": "latest.json", "browser_download_url": "u"}]}))


if __name__ == "__main__":
    unittest.main()
