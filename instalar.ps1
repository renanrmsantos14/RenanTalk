# Instala ou reinstala o RenanTalk a partir do último release do GitHub.
# Uso (PowerShell):  irm https://raw.githubusercontent.com/renanrmsantos14/RenanTalk/main/instalar.ps1 | iex
$ErrorActionPreference = 'Stop'
$repo = 'renanrmsantos14/RenanTalk'
$app = Join-Path $env:LOCALAPPDATA 'RenanTalk\app'
$startup = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\Startup'

# Python 3.10+ (o pythonw roda sem janela)
$python = (& py -3 -c "import sys; print(sys.executable)" 2>$null)
if (-not $python) { throw 'Python 3 não encontrado. Instale com: winget install Python.Python.3.13' }
$pythonw = Join-Path (Split-Path $python) 'pythonw.exe'

# Para a instância em execução (se houver)
Get-CimInstance Win32_Process -Filter "Name='pythonw.exe'" |
    Where-Object { $_.CommandLine -match 'ditado\.pyw' } |
    ForEach-Object { Stop-Process -Id $_.ProcessId -Force }

# Baixa e extrai o último release
$rel = Invoke-RestMethod "https://api.github.com/repos/$repo/releases/latest" -Headers @{ 'User-Agent' = 'renantalk-instalar' }
$asset = $rel.assets | Where-Object name -eq 'RenanTalk.zip'
$zip = Join-Path $env:TEMP 'RenanTalk.zip'
Invoke-WebRequest $asset.browser_download_url -OutFile $zip -UseBasicParsing
New-Item -ItemType Directory -Force $app | Out-Null
Expand-Archive $zip -DestinationPath $app -Force
Remove-Item $zip

# Dependências
& $python -m pip install --user -q -r (Join-Path $app 'requirements.txt')

# Inicia com o Windows (remove o atalho da versão antiga, se existir)
Remove-Item (Join-Path $startup 'Ditado Groq.lnk') -ErrorAction SilentlyContinue
$lnk = (New-Object -ComObject WScript.Shell).CreateShortcut((Join-Path $startup 'RenanTalk.lnk'))
$lnk.TargetPath = $pythonw
$lnk.Arguments = '"' + (Join-Path $app 'ditado.pyw') + '"'
$lnk.WorkingDirectory = $app
$lnk.IconLocation = Join-Path $env:APPDATA 'RenanTalk\renantalk.ico'
$lnk.Description = 'RenanTalk - ditado por voz (Ctrl+Win)'
$lnk.Save()

Start-Process $pythonw -ArgumentList ('"' + (Join-Path $app 'ditado.pyw') + '"') -WorkingDirectory $app
Write-Host "RenanTalk $($rel.tag_name) instalado em $app. Segure Ctrl+Win para ditar."
