# Teste da versão-ponte no Windows (workflow ponte.yml). Não publica nada e não usa a chave real.
# Entrada: $env:PACOTES com RenanTalk_<v>_x64-setup.exe e .sig (assinados com a chave de teste do job),
# $env:RENANTALK_TESTE_PUBKEY (pública de teste) e $env:VERSAO_RUST.
# Etapas: instala o Python atual pelo instalar.ps1 (apontado para um release falso local), semeia os dados, força
# 3 falhas (instalador corrompido) e depois a ponte boa; confere o que ficou em cada caso.
$ErrorActionPreference = 'Stop'
$raiz = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$porta = 38910
$srv = Join-Path $env:RUNNER_TEMP 'release-falso'
$dados = Join-Path $env:APPDATA 'RenanTalk'
$app = Join-Path $env:LOCALAPPDATA 'RenanTalk\app'
$rust = Join-Path $env:LOCALAPPDATA 'RenanTalk'
$lnk = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\Startup\RenanTalk.lnk'
$log = Join-Path $dados 'ditado.log'
$v = $env:VERSAO_RUST
$falhas = 0

function Conferir($ok, $texto) {
    if ($ok) { Write-Host "OK   $texto" } else { Write-Host "FALHA $texto"; $script:falhas++ }
}
function PythonVivo { @(Get-CimInstance Win32_Process -Filter "Name='pythonw.exe'" | Where-Object { $_.CommandLine -match 'ditado\.pyw' }) }
function RustVivo { @(Get-Process renantalk -ErrorAction SilentlyContinue) }
function Esperar($cond, $s, $texto) {
    $fim = (Get-Date).AddSeconds($s)
    while ((Get-Date) -lt $fim) { if (& $cond) { return $true }; Start-Sleep -Milliseconds 500 }
    Write-Host "(esperei $s s: $texto)"; return $false
}
function Log { if (Test-Path $log) { Get-Content $log -Encoding utf8 } else { @() } }
function IniciarPython {
    $pyw = (Get-Item (Join-Path (Split-Path (& py -3 -c "import sys; print(sys.executable)")) 'pythonw.exe')).FullName
    Start-Process $pyw -ArgumentList ('"' + (Join-Path $app 'ditado.pyw') + '"') -WorkingDirectory $app
}

# ---------- release falso: o zip do Python (código desta branch) e o latest.json ----------
New-Item -ItemType Directory -Force (Join-Path $srv 'releases') | Out-Null
$pkg = Join-Path $env:RUNNER_TEMP 'pacote-python'
Copy-Item -Recurse (Join-Path $raiz 'app') $pkg
Copy-Item (Join-Path $raiz 'requirements.txt') $pkg
Set-Content (Join-Path $pkg 'VERSION') '0.0.909 · teste' -Encoding utf8
Remove-Item -Recurse -Force (Join-Path $pkg '__pycache__') -ErrorAction SilentlyContinue
Compress-Archive -Path (Join-Path $pkg '*') -DestinationPath (Join-Path $srv 'RenanTalk.zip')
$setup = Get-ChildItem $env:PACOTES -Filter "RenanTalk_${v}_x64-setup.exe" | Select-Object -First 1
Copy-Item $setup.FullName (Join-Path $srv $setup.Name)
$bytes = [IO.File]::ReadAllBytes($setup.FullName); $bytes[$bytes.Length - 100] = $bytes[$bytes.Length - 100] -bxor 0xFF
[IO.File]::WriteAllBytes((Join-Path $srv "corrompido-$($setup.Name)"), $bytes)
$sig = (Get-Content "$($setup.FullName).sig" -Raw).Trim()
function Publicar($nome) {
    @{ version = $v; platforms = @{ 'windows-x86_64' = @{ url = "http://127.0.0.1:$porta/$nome"; signature = $sig } } } |
        ConvertTo-Json -Depth 5 | Set-Content (Join-Path $srv 'latest.json') -Encoding utf8
}
@{ tag_name = 'v0.0.909'; assets = @(
    @{ name = 'RenanTalk.zip'; browser_download_url = "http://127.0.0.1:$porta/RenanTalk.zip" },
    @{ name = 'latest.json'; browser_download_url = "http://127.0.0.1:$porta/latest.json" }) } |
    ConvertTo-Json -Depth 5 | Set-Content (Join-Path $srv 'releases\latest') -Encoding utf8
Publicar "corrompido-$($setup.Name)"
Start-Process python -ArgumentList '-m', 'http.server', $porta, '--bind', '127.0.0.1', '--directory', $srv -WindowStyle Hidden
Start-Sleep 2

# ---------- dados semeados antes de instalar ----------
New-Item -ItemType Directory -Force $dados | Out-Null
Set-Content (Join-Path $dados 'chave.txt') 'gsk_teste_falsa_0000000000000000000000000000000000000000' -NoNewline -Encoding ascii
Set-Content (Join-Path $dados 'vocabulario.txt') "Betinhos`nPonte de teste`n" -NoNewline -Encoding utf8
Set-Content (Join-Path $dados 'ajustes.json') '{"volume": 37, "notificacoes": true}' -Encoding ascii
& py -3 -c @"
import os, sys, sqlite3
sys.path.insert(0, r'$raiz\app')
import historico  # só sqlite: roda antes do pip do instalar.ps1
for t in ('primeiro ditado da ponte', 'segundo ditado da ponte'):
    historico.gravar(t, app='teste', idioma='pt', duracao_s=1.5)
print('historico:', sqlite3.connect(os.path.join(r'$dados', 'historico.db')).execute('select count(*) from ditado').fetchone()[0])
"@
$antes = @{}
foreach ($f in 'chave.txt', 'vocabulario.txt', 'historico.db', 'ajustes.json') { $antes[$f] = (Get-FileHash (Join-Path $dados $f)).Hash }

# ---------- Python atual pelo instalar.ps1 (adaptado: release falso local, sem interação) ----------
$inst = Join-Path $env:RUNNER_TEMP 'instalar-teste.ps1'
(Get-Content (Join-Path $raiz 'instalar.ps1') -Raw -Encoding utf8).Replace(
    'https://api.github.com/repos/$repo/releases/latest', "http://127.0.0.1:$porta/releases/latest") |
    Set-Content $inst -Encoding utf8
& $inst
Conferir (Test-Path (Join-Path $app 'ditado.pyw')) "Python instalado em $app"
Conferir (Test-Path $lnk) 'Startup\RenanTalk.lnk criado'

# ---------- falha: instalador corrompido, 3 vezes seguidas ----------
foreach ($n in 1..3) {
    if ($n -gt 1) {  # a próxima verificação viria em 6 h: reinicia o Python para tentar já
        PythonVivo | ForEach-Object { Stop-Process -Id $_.ProcessId -Force }; Start-Sleep 1; IniciarPython
    }
    $ok = Esperar { (Log | Select-String "falha $n seguida").Count -gt 0 } 90 "falha $n"
    Conferir $ok "ponte falhou (falha $n seguida no log)"
}
Start-Sleep 2
Write-Host '--- log (falhas) ---'; Log | Select-String 'ponte|iniciado' | Select-Object -Last 14 | ForEach-Object { $_.Line }
Conferir ((Get-Content (Join-Path $dados '.ponte-falhas')) -eq '3') 'contador .ponte-falhas = 3'
Conferir ((Log | Select-String 'assinatura|não confere').Count -gt 0) 'motivo: assinatura não confere'
Conferir ((Log | Select-String 'aviso por notificação \(3 falhas').Count -gt 0) 'aviso da 3ª falha'
Conferir ((Log | Select-String 'ditado do Python religado').Count -ge 3) 'gancho do Python religado'
Conferir ((PythonVivo).Count -eq 1) 'Python segue rodando'
Conferir ((RustVivo).Count -eq 0) 'app Rust não ficou rodando'
Conferir (Test-Path (Join-Path $app 'ditado.pyw')) 'pasta app\ intacta'
Conferir (Test-Path $lnk) 'Startup\RenanTalk.lnk intacto'
Conferir (-not (Test-Path (Join-Path $rust 'renantalk.exe'))) 'nada do Rust instalado (o pacote ruim nem rodou)'

# ---------- sucesso ----------
Publicar $setup.Name
PythonVivo | ForEach-Object { Stop-Process -Id $_.ProcessId -Force }; Start-Sleep 1; IniciarPython
$pid0 = (PythonVivo)[0].ProcessId
$ok = Esperar { (Log | Select-String 'ponte: Python encerrado').Count -gt 0 } 240 'ponte confirmada'
Conferir $ok 'ponte confirmada e Python encerrado'
Esperar { -not (Test-Path $app) } 150 'app\ removida' | Out-Null
Start-Sleep 2
Write-Host '--- limpeza destacada (%TEMP%\RenanTalk-ponte-limpeza.log) ---'
Get-Content (Join-Path $env:TEMP 'RenanTalk-ponte-limpeza.log') -ErrorAction SilentlyContinue | Select-Object -Last 15
Get-CimInstance Win32_Process -Filter "Name='powershell.exe'" | ForEach-Object { "powershell vivo: $($_.ProcessId) $($_.CommandLine)" }
Write-Host '--- Run (HKCU e HKLM) ---'
foreach ($r in 'HKCU', 'HKLM') { "$r Run:"; Get-ItemProperty "${r}:\Software\Microsoft\Windows\CurrentVersion\Run" | Format-List }
Write-Host '--- log (sucesso) ---'; Log | Select-Object -Last 22
$m = Get-Content (Join-Path $dados '.app-subiu.json') -Raw | ConvertFrom-Json
Write-Host "marcador: $($m | ConvertTo-Json -Compress)"
Conferir ($m.versao -eq $v -and $m.gancho -and $m.autostart) "marcador .app-subiu.json com v$v, gancho e autostart"
Conferir ((RustVivo).Count -ge 1) "app Rust rodando ($((RustVivo).Path -join ', '))"
Conferir (-not (Get-Process -Id $pid0 -ErrorAction SilentlyContinue)) 'Python encerrado'
Conferir (-not (Test-Path $lnk)) 'Startup\RenanTalk.lnk removido'
Conferir (-not (Test-Path $app)) "$app removida"
Conferir (Test-Path (Join-Path $rust 'renantalk.exe')) 'renantalk.exe do NSIS intacto na pasta-mãe'
# o auto-launch grava em HKLM quando o processo é elevado (o runner é admin sem UAC); sem elevação, em HKCU
$run = foreach ($r in 'HKCU', 'HKLM') {
    $x = Get-ItemProperty "${r}:\Software\Microsoft\Windows\CurrentVersion\Run" -Name RenanTalk -ErrorAction SilentlyContinue
    if ($x) { "$r = $($x.RenanTalk)" } }
Conferir ($null -ne $run) "autostart do Rust (Run: $run)"
foreach ($f in $antes.Keys) {
    $igual = (Get-FileHash (Join-Path $dados $f)).Hash -eq $antes[$f]
    if ($f -eq 'ajustes.json') { $igual = $igual -or ((Get-Content (Join-Path $dados $f) -Raw | ConvertFrom-Json).volume -eq 37) }
    Conferir $igual "dados intactos: $f"
}
Conferir (-not (Test-Path (Join-Path $dados '.ponte-falhas'))) 'contador zerado'

# ---------- desinstalador do NSIS: não leva os dados ----------
Get-Process renantalk -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Process (Join-Path $rust 'uninstall.exe') -ArgumentList '/S' -Wait
Esperar { -not (Test-Path (Join-Path $rust 'renantalk.exe')) } 30 'desinstalar' | Out-Null
Conferir (-not (Test-Path (Join-Path $rust 'renantalk.exe'))) 'desinstalador removeu o renantalk.exe'
Conferir ((Get-FileHash (Join-Path $dados 'chave.txt')).Hash -eq $antes['chave.txt']) 'desinstalador não tocou em %APPDATA%\RenanTalk'

if ($falhas) { throw "$falhas conferência(s) falharam" }
Write-Host 'ponte no Windows: tudo conferido'
