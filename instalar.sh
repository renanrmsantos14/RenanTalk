#!/usr/bin/env bash
# Instala ou reinstala o RenanTalk no Linux (X11; testado em Linux Mint / Cinnamon), sem sudo.
# Uso:  curl -fsSL https://raw.githubusercontent.com/renanrmsantos14/RenanTalk/main/instalar.sh | bash
set -euo pipefail
REPO="renanrmsantos14/RenanTalk"
APP="$HOME/.local/share/RenanTalk/app"
AUTOSTART="$HOME/.config/autostart/renantalk.desktop"

[ "${XDG_SESSION_TYPE:-x11}" = "wayland" ] && echo "Aviso: sessão Wayland. O RenanTalk precisa de X11 para o atalho global e a luz."
command -v python3 >/dev/null || { echo "Python 3 não encontrado."; exit 1; }
python3 -c "import gi; gi.require_version('Gtk','3.0')" 2>/dev/null || { echo "Falta o GTK para Python: sudo apt install python3-gi gir1.2-gtk-3.0"; exit 1; }

# pip só para o usuário (o Mint não traz pip por padrão)
if ! python3 -m pip --version >/dev/null 2>&1; then
  curl -fsSL https://bootstrap.pypa.io/get-pip.py -o /tmp/get-pip.py
  python3 /tmp/get-pip.py --user --break-system-packages -q
  rm -f /tmp/get-pip.py
fi

# Para a instância em execução (se houver)
pkill -f "RenanTalk/app/ditado.pyw" 2>/dev/null || true

# Baixa e extrai o último release
URL=$(curl -fsSL -H "User-Agent: renantalk-instalar" "https://api.github.com/repos/$REPO/releases/latest" \
  | python3 -c "import json,sys; r=json.load(sys.stdin); print(next(a['browser_download_url'] for a in r['assets'] if a['name']=='RenanTalk-linux.zip'))")
mkdir -p "$APP"
curl -fsSL "$URL" -o /tmp/RenanTalk.zip
python3 -c "import zipfile,sys; zipfile.ZipFile(sys.argv[1]).extractall(sys.argv[2])" /tmp/RenanTalk.zip "$APP"
chmod +x "$APP/renantalk-config" 2>/dev/null || true
rm -f /tmp/RenanTalk.zip

# Dependências (só no seu usuário)
python3 -m pip install --user --break-system-packages -q -r "$APP/requirements.txt"

# Inicia com a sessão
mkdir -p "$(dirname "$AUTOSTART")"
cat > "$AUTOSTART" <<EOF
[Desktop Entry]
Type=Application
Name=RenanTalk
Comment=Ditado por voz (segure Ctrl+Super)
Exec=python3 $APP/ditado.pyw
Icon=$HOME/.config/RenanTalk/renantalk.png
X-GNOME-Autostart-enabled=true
NoDisplay=false
EOF

nohup python3 "$APP/ditado.pyw" >/dev/null 2>&1 &
echo "RenanTalk instalado em $APP. Segure Ctrl+Super para ditar."
