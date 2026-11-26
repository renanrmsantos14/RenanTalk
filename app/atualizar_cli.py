"""Procura atualização agora (chamado pela janela de configurações em Rust).

Uso: python atualizar_cli.py <pid_do_ditado>
Imprime {"situacao": ..., "detalhe": ...} em JSON. Se atualizou, encerra o ditado em execução
e abre a versão nova.
"""
import json
import os
import subprocess
import sys
import time

APP = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, APP)

import atualizador  # noqa: E402
from comum import ARQ_LOG  # noqa: E402

pid = int(sys.argv[1]) if len(sys.argv) > 1 and sys.argv[1].isdigit() else 0
situacao, detalhe = atualizador.verificar_e_aplicar()
with open(ARQ_LOG, "a", encoding="utf-8") as f:
    f.write(time.strftime("%d/%m/%Y %H:%M:%S ") + f"atualização: {situacao} {detalhe} (manual)\n")
print(json.dumps({"situacao": situacao, "detalhe": detalhe}, ensure_ascii=False), flush=True)

if situacao == "atualizado":
    if pid:
        try:
            os.kill(pid, 9)  # encerra o ditado antigo; a trava de instância é liberada junto
        except OSError:
            pass
        time.sleep(0.5)
    python = sys.executable
    if sys.platform == "win32":
        python = os.path.join(os.path.dirname(sys.executable), "pythonw.exe")  # sem janela de console
    # sem herdar a saída de quem chamou: senão um "| tail" no terminal fica preso até o app fechar
    subprocess.Popen([python, os.path.join(APP, "ditado.pyw")], cwd=APP, stdin=subprocess.DEVNULL,
                     stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=sys.platform != "win32")
