"""Versão-ponte: troca este ditado em Python pelo app novo em Rust (apps/desktop) e se remove.

Roda dentro do ditado (`ditado.pyw`), na verificação de atualização (60 s depois de subir e a cada 6 h):
1. o release tem `latest.json` com o instalador desta plataforma (NSIS no Windows, AppImage no Linux);
2. baixa e confere a assinatura minisign com a chave pública do app (a mesma `pubkey` do tauri.conf.json);
3. o ditado do Python já está pausado (quem chama pausa antes), para não haver dois ditados juntos;
4. instala (NSIS silencioso por usuário; AppImage em ~/.local/share/RenanTalk) e inicia o app novo;
5. espera até 60 s o marcador `.app-subiu.json` que o app novo grava ao subir com o gancho de teclado ligado;
6. confirmou: remove o início automático e a pasta de código do Python (nunca a pasta de dados) e devolve True;
7. falhou em qualquer etapa: encerra o app novo, desfaz o início automático dele, conta a falha e devolve False
   (quem chama religa o ditado do Python); na 3ª falha seguida (e a cada 3) avisa por notificação.

Só para teste (nunca nas máquinas): RENANTALK_TESTE_API troca a URL do `releases/latest` e
RENANTALK_TESTE_PUBKEY troca a chave pública (formato do tauri.conf.json). Roteiro: .claude/skills/verificar-ponte.
"""
import json
import os
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import urllib.request

import minisign
from comum import APP_DIR, PASTA, WINDOWS, definir_inicio_automatico, log

# Chave pública do app em Rust (apps/desktop/src-tauri/tauri.conf.json, plugins.updater.pubkey); o teste
# tests/test_ponte.py confere que é a mesma.
PUBKEY = ("dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDJBMDQ3NUQ1RjM2MEQ4QTUKUldTbDJHRHoxWFVFS3V2NVFlek8zK3p"
          "ha1ArWk9LM2c3QnVOWUJkUmVxWFZLVzIzVTRmNHpIUlUK")
PLATAFORMA = "windows-x86_64" if WINDOWS else "linux-x86_64"
MARCADOR = ".app-subiu.json"  # gravado pelo app Rust (casca::marcar_subiu)
FALHAS = ".ponte-falhas"
MARCA_AUTOSTART_RUST = ".autostart-feito"  # casca::autostart_primeira_vez: sem ela o app novo liga o autostart de novo
ESPERA_S = 60
AVISAR_A_CADA = 3
SEM_JANELA = 0x08000000  # CREATE_NO_WINDOW
DESTACADO = 0x00000008 | 0x00000200  # DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP

if WINDOWS:
    _LOCAL = os.environ.get("LOCALAPPDATA", "")
    DIR_RUST = os.path.join(_LOCAL, "RenanTalk")  # padrão do NSIS por usuário ($LOCALAPPDATA\${PRODUCTNAME})
    APP_INSTALADOR = os.path.join(DIR_RUST, "app")  # instalar.ps1
    EXE_RUST = os.path.join(DIR_RUST, "renantalk.exe")
else:
    DIR_RUST = os.path.join(os.path.expanduser("~/.local/share"), "RenanTalk")
    APP_INSTALADOR = os.path.join(DIR_RUST, "app")  # instalar.sh
    EXE_RUST = os.path.join(DIR_RUST, "RenanTalk.AppImage")
    # tauri-plugin-autostart (auto-launch): <config>/autostart/<productName>.desktop
    AUTOSTART_RUST = os.path.join(os.path.expanduser("~/.config/autostart"),  # o plugin ignora XDG_CONFIG_HOME
                                  "RenanTalk.desktop")


class Falha(Exception):
    def __init__(self, etapa: str, detalhe: str) -> None:
        super().__init__(f"{etapa}: {detalhe}")
        self.etapa = etapa


def _baixar(url: str, timeout: int = 120) -> bytes:
    import atualizador

    with urllib.request.urlopen(urllib.request.Request(url, headers=atualizador.CAB), timeout=timeout) as r:
        return r.read()


def procurar(rel: dict) -> dict | None:
    """{versao, url, assinatura} do app Rust desta plataforma no release, ou None (sem `latest.json` ou sem ela)."""
    asset = next((a for a in rel.get("assets", []) if a.get("name") == "latest.json"), None)
    if not asset:
        return None
    try:
        info = json.loads(_baixar(asset["browser_download_url"], 60))
        p = info["platforms"][PLATAFORMA]
        return {"versao": str(info["version"]).lstrip("v"), "url": p["url"], "assinatura": p["signature"]}
    except (KeyError, TypeError, ValueError):
        return None


# ---------- contador de falhas ----------
def falhas_seguidas(pasta: str | None = None) -> int:
    pasta = pasta or PASTA
    try:
        with open(os.path.join(pasta, FALHAS), encoding="utf-8") as f:
            return int(f.read().strip() or 0)
    except (OSError, ValueError):
        return 0


def registrar_falha(pasta: str | None = None) -> int:
    pasta = pasta or PASTA
    n = falhas_seguidas(pasta) + 1
    try:
        with open(os.path.join(pasta, FALHAS), "w", encoding="utf-8") as f:
            f.write(str(n))
    except OSError:
        pass
    return n


def deve_avisar(n: int) -> bool:
    return n > 0 and n % AVISAR_A_CADA == 0


# ---------- confirmação ----------
def confirmado(versao: str, pasta: str | None = None) -> bool:
    """O app novo subiu de verdade: o marcador tem a versão instalada, o gancho e o início automático ligados."""
    pasta = pasta or PASTA
    try:
        with open(os.path.join(pasta, MARCADOR), encoding="utf-8") as f:
            m = json.load(f)
    except (OSError, ValueError):
        return False
    return (isinstance(m, dict) and str(m.get("versao", "")).lstrip("v") == versao and m.get("gancho") is True
            and m.get("autostart") is True)


def esperar_confirmacao(versao: str, proc, espera: float = ESPERA_S, pasta: str | None = None) -> None:
    fim = time.monotonic() + espera
    while time.monotonic() < fim:
        if confirmado(versao, pasta):
            if proc is None or proc.poll() is None:
                return
        if proc is not None and proc.poll() is not None:
            raise Falha("confirmação", f"o app novo fechou sozinho (código {proc.returncode})")
        time.sleep(0.5)
    raise Falha("confirmação", f"sem o marcador {MARCADOR} da v{versao} em {espera:.0f} s")


# ---------- app novo ----------
def _encerrar_rust_existente() -> None:
    """Um app novo que sobrou de uma tentativa anterior (sem confirmação) é encerrado antes de instalar por cima."""
    if WINDOWS:
        subprocess.run(["taskkill", "/F", "/IM", "renantalk.exe"], capture_output=True, creationflags=SEM_JANELA)
    else:
        subprocess.run(["pkill", "-x", "-u", str(os.getuid()), "renantalk"], capture_output=True)
    time.sleep(1)


def instalar(dados: bytes) -> str:
    """Instala o pacote já conferido e devolve o executável."""
    os.makedirs(DIR_RUST, exist_ok=True)
    if WINDOWS:
        setup = os.path.join(tempfile.gettempdir(), "RenanTalk-ponte-setup.exe")
        with open(setup, "wb") as f:
            f.write(dados)
        try:
            r = subprocess.run([setup, "/S"], timeout=600, creationflags=SEM_JANELA)
        except (OSError, subprocess.SubprocessError) as e:
            raise Falha("instalação", repr(e)) from e
        finally:
            try:
                os.remove(setup)
            except OSError:
                pass
        if r.returncode != 0:
            raise Falha("instalação", f"o instalador NSIS saiu com {r.returncode}")
    else:
        novo = EXE_RUST + ".novo"
        with open(novo, "wb") as f:
            f.write(dados)
        os.chmod(novo, 0o755)
        os.replace(novo, EXE_RUST)
    if not os.path.isfile(EXE_RUST):
        raise Falha("instalação", f"não achei {EXE_RUST} depois de instalar")
    return EXE_RUST


def iniciar(exe: str):
    saida = dict(stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, close_fds=True)
    if WINDOWS:
        return subprocess.Popen([exe], cwd=os.path.dirname(exe), creationflags=DESTACADO, **saida)
    return subprocess.Popen([exe], cwd=os.path.expanduser("~"), start_new_session=True, **saida)


def _encerrar(proc) -> None:
    if proc is None or proc.poll() is not None:
        return
    try:
        if WINDOWS:
            subprocess.run(["taskkill", "/F", "/T", "/PID", str(proc.pid)], capture_output=True, creationflags=SEM_JANELA)
        else:
            os.killpg(proc.pid, signal.SIGTERM)  # o AppImage é um processo da FUSE com o app como filho
            try:
                proc.wait(5)
            except subprocess.TimeoutExpired:
                os.killpg(proc.pid, signal.SIGKILL)
    except OSError:
        pass
    _encerrar_rust_existente()


def desfazer_autostart_rust(pasta: str | None = None) -> None:
    """Sem isso, no próximo boot subiriam os dois ditados. A marca sai junto para a próxima tentativa religar."""
    pasta = pasta or PASTA
    if WINDOWS:
        import winreg

        # o auto-launch tenta HKLM primeiro (processo elevado) e cai para HKCU (o normal, sem elevação)
        for raiz in (winreg.HKEY_CURRENT_USER, winreg.HKEY_LOCAL_MACHINE):
            try:
                with winreg.OpenKey(raiz, r"Software\Microsoft\Windows\CurrentVersion\Run", 0,
                                    winreg.KEY_SET_VALUE) as k:
                    winreg.DeleteValue(k, "RenanTalk")
            except OSError:
                pass
    else:
        try:
            os.remove(AUTOSTART_RUST)
        except OSError:
            pass
    for nome in (MARCA_AUTOSTART_RUST, MARCADOR):
        try:
            os.remove(os.path.join(pasta, nome))
        except OSError:
            pass


# ---------- remoção do Python ----------
def _mesma(a: str, b: str) -> bool:
    return os.path.normcase(os.path.realpath(a)) == os.path.normcase(os.path.realpath(b))


def _dentro(a: str, b: str) -> bool:
    """`a` é `b` ou fica dentro dela."""
    a, b = (os.path.normcase(os.path.realpath(x)) for x in (a, b))
    try:
        return os.path.commonpath([a, b]) == b
    except ValueError:  # discos diferentes no Windows
        return False


def remover_python(app_dir: str = APP_DIR) -> None:
    """Só depois da confirmação. Tira o início automático do Python e a pasta de código que o instalador criou.

    Nunca toca na pasta de dados (`PASTA`) nem nos arquivos do NSIS/AppImage (`DIR_RUST`). Python rodando de outro
    lugar (um clone do repositório, por exemplo) não tem a pasta apagada."""
    definir_inicio_automatico(False)  # Startup\RenanTalk.lnk ou ~/.config/autostart/renantalk.desktop
    log("ponte: início automático do Python removido")
    try:
        os.remove(os.path.join(PASTA, FALHAS))
    except OSError:
        pass
    seguro = (_mesma(app_dir, APP_INSTALADOR) and os.path.isfile(os.path.join(app_dir, "ditado.pyw"))
              and not _dentro(PASTA, app_dir) and not _dentro(app_dir, PASTA) and not _dentro(DIR_RUST, app_dir))
    if not seguro:
        log(f"ponte: o Python roda fora da pasta do instalador ({app_dir}); a pasta não foi apagada")
        return
    if WINDOWS:
        _apagar_depois_de_sair(app_dir)
    else:
        shutil.rmtree(app_dir, ignore_errors=True)
        log(f"ponte: pasta do Python removida ({app_dir})" if not os.path.exists(app_dir)
            else f"ponte: não consegui remover {app_dir}")


def _apagar_depois_de_sair(app_dir: str) -> None:
    """No Windows a pasta em uso (diretório atual, janela antiga) não sai: um PowerShell destacado espera este
    processo terminar e apaga. O roteiro e o registro dele ficam em %TEMP% (RenanTalk-ponte-limpeza.*)."""
    q = lambda s: "'" + s.replace("'", "''") + "'"  # noqa: E731
    temp = os.environ.get("TEMP") or tempfile.gettempdir()
    roteiro, registro = (os.path.join(temp, f"RenanTalk-ponte-limpeza.{x}") for x in ("ps1", "log"))
    with open(roteiro, "w", encoding="utf-8-sig") as f:  # com BOM: o PowerShell 5.1 lê os acentos certos
        f.write(f"""$ErrorActionPreference = 'SilentlyContinue'
Start-Transcript -LiteralPath {q(registro)} -Force | Out-Null
Wait-Process -Id {os.getpid()} -Timeout 120
for ($i = 0; $i -lt 30; $i++) {{
    Remove-Item -LiteralPath {q(app_dir)} -Recurse -Force
    if (-not (Test-Path -LiteralPath {q(app_dir)})) {{ break }}
    Start-Sleep 1
}}
$m = if (Test-Path -LiteralPath {q(app_dir)}) {{ 'ponte: não consegui remover ' + {q(app_dir)} }} else {{ 'ponte: pasta do Python removida (' + {q(app_dir)} + ')' }}
$m
Add-Content -LiteralPath {q(os.path.join(PASTA, 'ditado.log'))} -Encoding utf8 -Value ((Get-Date -Format 'dd/MM/yyyy HH:mm:ss') + ' ' + $m)
Stop-Transcript | Out-Null
""")
    p = subprocess.Popen(["powershell", "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-WindowStyle",
                          "Hidden", "-File", roteiro], cwd=temp, creationflags=DESTACADO | SEM_JANELA,
                         stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, close_fds=True)
    os.chdir(temp)  # este processo deixa de prender a pasta
    log(f"ponte: a pasta do Python ({app_dir}) sai quando este processo fechar (limpeza pid {p.pid}, {registro})")


# ---------- fluxo ----------
def pubkey() -> str:
    return os.environ.get("RENANTALK_TESTE_PUBKEY") or PUBKEY


def executar(alvo: dict, notificar=lambda _t: None) -> bool:
    """Instala, inicia e espera o app novo; True = confirmado e Python removido (quem chama sai).

    Quem chama já pausou o ditado do Python; com False, religa."""
    versao, proc = alvo["versao"], None
    try:
        log(f"ponte: baixando o app novo v{versao} ({alvo['url'].rsplit('/', 1)[-1]})")
        try:
            dados = _baixar(alvo["url"], 300)
        except Exception as e:  # noqa: BLE001
            raise Falha("download", repr(e)) from e
        try:
            minisign.conferir(dados, alvo["assinatura"], pubkey())
        except minisign.AssinaturaInvalida as e:
            raise Falha("integridade", str(e)) from e
        log(f"ponte: assinatura confere ({len(dados)} bytes)")
        _encerrar_rust_existente()
        try:
            os.remove(os.path.join(PASTA, MARCADOR))
        except OSError:
            pass
        exe = instalar(dados)
        log(f"ponte: instalado em {exe}")
        try:
            proc = iniciar(exe)
        except OSError as e:
            raise Falha("início", repr(e)) from e
        log(f"ponte: app novo iniciado (pid {proc.pid}); esperando a confirmação por até {ESPERA_S} s")
        esperar_confirmacao(versao, proc)
    except Exception as e:  # noqa: BLE001 - qualquer falha: o Python segue
        etapa = e.etapa if isinstance(e, Falha) else "inesperada"
        _encerrar(proc)
        desfazer_autostart_rust()
        n = registrar_falha()
        log(f"ponte: falhou ({e}); falha {n} seguida; o Python segue ditando e tenta de novo na próxima verificação")
        if deve_avisar(n):
            log(f"ponte: aviso por notificação ({n} falhas seguidas)")
            notificar(f"Não consegui instalar o RenanTalk novo ({n} tentativas, etapa: {etapa}). "
                      "O ditado continua funcionando; detalhes no log.")
        return False
    log(f"ponte: app novo confirmou v{versao}; removendo o Python")
    remover_python()
    return True
