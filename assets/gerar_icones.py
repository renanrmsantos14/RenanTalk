"""Gera os ícones do RenanTalk a partir de assets/icone.svg e dos ícones do menu (rodar só quando o desenho mudar).

Rasteriza os SVG num Edge/Chrome headless (canvas) e grava:
- app/icones/renantalk.ico (16 a 256), app/icones/renantalk.png (256) e renantalk-64.png
- config/icons/icon.ico e icon.png (janela Tauri)
- app/icones/menu-<nome>-<tam>.bgra: bitmap BGRA pré-multiplicado, linhas de cima para baixo (menu do Windows)
Uso: python assets/gerar_icones.py  (precisa do Edge e do Node)
"""
import base64
import json
import os
import shutil
import struct
import subprocess
import tempfile
import time
import urllib.request

RAIZ = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SVG = open(os.path.join(RAIZ, "assets", "icone.svg"), encoding="utf-8").read()
_TRACO = "fill='none' stroke='{c}' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'"
MENU = {  # ícones de linha 24x24 (estilo Lucide)
    "config": ("#2563eb", "<circle cx='12' cy='12' r='3'/><path d='M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z'/>"),
    "atualizar": ("#4b5563", "<path d='M21 12a9 9 0 1 1-3-6.7L21 8'/><path d='M21 3v5h-5'/>"),
    "log": ("#4b5563", "<path d='M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z'/><path d='M14 3v5h5M9 13h6M9 17h6'/>"),
    "sair": ("#dc2626", "<path d='M12 3v9'/><path d='M6.3 6.3a8 8 0 1 0 11.4 0'/>"),
}


def svg_menu(nome: str) -> str:
    cor, corpo = MENU[nome]
    return f"<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' {_TRACO.format(c=cor)}>{corpo}</svg>"


PAGINA = """<!doctype html><canvas id=c></canvas><script>
async function rasterizar(svg, n) {
  const img = new Image(); img.src = 'data:image/svg+xml;base64,' + btoa(unescape(encodeURIComponent(svg)));
  await img.decode();
  const c = document.getElementById('c'); c.width = n; c.height = n;
  const x = c.getContext('2d'); x.clearRect(0, 0, n, n); x.imageSmoothingQuality = 'high'; x.drawImage(img, 0, 0, n, n);
  const d = x.getImageData(0, 0, n, n).data; let bin = ''; for (let i = 0; i < d.length; i++) bin += String.fromCharCode(d[i]);
  return {png: c.toDataURL('image/png').split(',')[1], rgba: btoa(bin)};
}
</script>"""

CDP_JS = r"""
const ws = new WebSocket(process.argv[2]); let id = 0; const pend = {};
const cmd = (m, p = {}) => new Promise(r => { const i = ++id; pend[i] = r; ws.send(JSON.stringify({id: i, method: m, params: p})); });
ws.onmessage = m => { const d = JSON.parse(m.data); if (pend[d.id]) pend[d.id](d.result); };
ws.onopen = async () => {
  const pedidos = JSON.parse(require('fs').readFileSync(process.argv[3], 'utf8')); const saida = {};
  for (const [chave, svg, n] of pedidos) {
    const r = await cmd('Runtime.evaluate', {expression: `rasterizar(${JSON.stringify(svg)}, ${n})`, awaitPromise: true, returnByValue: true});
    saida[chave] = r.result.value;
  }
  require('fs').writeFileSync(process.argv[4], JSON.stringify(saida)); process.exit(0);
};
"""


def achar_navegador() -> str:
    for p in (r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe", r"C:\Program Files\Microsoft\Edge\Application\msedge.exe"):
        if os.path.exists(p):
            return p
    return shutil.which("brave-browser") or shutil.which("chromium") or shutil.which("google-chrome")


def ico(pngs: list[tuple[int, bytes]]) -> bytes:
    off = 6 + 16 * len(pngs)
    cab, ent = struct.pack("<HHH", 0, 1, len(pngs)), b""
    for t, p in pngs:
        ent += struct.pack("<BBBBHHII", t % 256, t % 256, 0, 0, 1, 32, len(p), off)
        off += len(p)
    return cab + ent + b"".join(p for _, p in pngs)


def bgra_premultiplicado(rgba: bytes) -> bytes:
    out = bytearray(len(rgba))
    for i in range(0, len(rgba), 4):
        r, g, b, a = rgba[i:i + 4]
        out[i:i + 4] = bytes((b * a // 255, g * a // 255, r * a // 255, a))
    return bytes(out)


def main() -> None:
    tmp = tempfile.mkdtemp()
    open(os.path.join(tmp, "p.html"), "w", encoding="utf-8").write(PAGINA)
    tam_ico = [16, 20, 24, 32, 40, 48, 64, 128, 256]
    pedidos = [[f"ico{t}", SVG, t] for t in tam_ico]
    pedidos += [[f"menu-{n}-{t}", svg_menu(n), t] for n in MENU for t in (16, 20, 24, 32)]
    pedidos += [[f"marca-{t}", SVG, t] for t in (16, 20, 24, 32)]  # cabeçalho do menu
    json.dump(pedidos, open(os.path.join(tmp, "pedidos.json"), "w"))
    open(os.path.join(tmp, "cdp.js"), "w").write(CDP_JS)
    nav = subprocess.Popen([achar_navegador(), "--headless=new", "--remote-debugging-port=9399",
                            f"--user-data-dir={os.path.join(tmp, 'perfil')}", "file:///" + os.path.join(tmp, "p.html").replace("\\", "/")])
    try:
        for _ in range(50):
            try:
                alvo = next(x for x in json.load(urllib.request.urlopen("http://127.0.0.1:9399/json")) if x["type"] == "page")
                break
            except Exception:  # noqa: BLE001
                time.sleep(0.2)
        time.sleep(0.5)
        subprocess.run(["node", os.path.join(tmp, "cdp.js"), alvo["webSocketDebuggerUrl"], os.path.join(tmp, "pedidos.json"),
                        os.path.join(tmp, "saida.json")], check=True, timeout=120)
    finally:
        nav.kill()
    r = json.load(open(os.path.join(tmp, "saida.json")))
    png = {t: base64.b64decode(r[f"ico{t}"]["png"]) for t in tam_ico}
    ic = os.path.join(RAIZ, "app", "icones")
    os.makedirs(ic, exist_ok=True)
    arq_ico = ico([(t, png[t]) for t in tam_ico])
    open(os.path.join(ic, "renantalk.ico"), "wb").write(arq_ico)
    open(os.path.join(ic, "renantalk.png"), "wb").write(png[256])
    open(os.path.join(ic, "renantalk-64.png"), "wb").write(png[64])
    open(os.path.join(RAIZ, "config", "icons", "icon.ico"), "wb").write(arq_ico)
    open(os.path.join(RAIZ, "config", "icons", "icon.png"), "wb").write(png[256])
    for chave, v in r.items():
        if chave.startswith(("menu-", "marca-")):
            open(os.path.join(ic, chave + ".bgra"), "wb").write(bgra_premultiplicado(base64.b64decode(v["rgba"])))
    print("ícones gerados em", ic)


if __name__ == "__main__":
    main()
