---
name: verificar-ponte
description: "Verifica a versão-ponte do RenanTalk (o Python que instala o app em Rust e se remove): assinatura minisign, instalação, marcador .app-subiu.json, remoção só com confirmação, falha com contador e aviso na 3ª. Use após mudar app/ponte.py, app/minisign.py, app/atualizador.py, o atualizar()/migrar() do app/ditado.pyw, o marcar_subiu do Rust, instalar.sh/instalar.ps1 ou .github/ponte/**."
---
# Verificar a versão-ponte

## Cobre
- `app/ponte.py` (fluxo), `app/minisign.py` (Ed25519 + BLAKE2b, só stdlib), `app/atualizador.py` (`ler_release`, `RENANTALK_TESTE_API`), `migrar`/`atualizar` em `app/ditado.pyw`, `notificar(forcar=True)` em `app/bandeja.py` e `app/plataforma_linux.py`.
- Rust: `casca::marcar_subiu` grava `<dados>/.app-subiu.json` (`versao`, `pid`, `gancho`, `autostart`) quando o gancho escuta (`pronto` em `plataforma/linux.rs` e `windows.rs`).
- `.github/workflows/ponte.yml` + `.github/ponte/teste-windows.ps1` (Windows limpo no CI).
- Ordem em `atualizar()`: o zip do Python primeiro (um `ponte.py` corrigido chega por ele), a ponte só com o Python em dia.
- `pausar` só depois do download e da assinatura (que tem de citar `file:<arquivo anunciado>`): falha de rede ou de assinatura não para o ditado.
- `ponte.retomar` no início do Python: com `.ponte-em-curso` (gravado antes de instalar) e o autostart do Rust ligado, termina a remoção se o app novo roda (pid do marcador com comm `renantalk`) ou desfaz o autostart dele. Sem `.ponte-em-curso` não faz nada (app Rust de dev com o perfil real).

## Regras de segurança do teste
- Nunca a chave real (`~/.config/RenanTalk-desktop-assinatura/`): gere uma de teste (`pnpm --filter desktop exec tauri signer generate --ci -w <scratchpad>/teste.key -p ""`).
- Nunca a tela real: Xvfb `:99`, `HOME` e `XDG_CONFIG_HOME` no scratchpad. Antes, `pgrep -x renantalk` e `pgrep -f RenanTalk/app/ditado.pyw` vazios (a ponte faz `pkill -x renantalk` do usuário).
- Variáveis só de teste: `RENANTALK_TESTE_API` (releases/latest falso), `RENANTALK_TESTE_PUBKEY` (pública de teste), `RENANTALK_TESTE_ATRASO` (s até a 1ª verificação; padrão 60), `RENANTALK_UPDATE_URL` (updater do Rust para um endereço morto).
- O build de teste roda o `bump-version.mjs`: depois, `git checkout -- apps/desktop/package.json apps/desktop/src/version.json apps/desktop/src-tauri/tauri.conf.json apps/desktop/src-tauri/Cargo.toml Cargo.lock`.

## Roteiro
1. `python3 -m unittest discover -s tests` → `OK` (os de `tests/test_ponte.py` incluídos: `Fluxo`, `Retomar`, `Remocao`).
2. AppImage de teste: `RENANTALK_VERSAO=0.0.910`, `TAURI_SIGNING_PRIVATE_KEY` de teste, `pnpm tauri build --bundles appimage --config '{"plugins":{"updater":{"pubkey":"<pública de teste>","dangerousInsecureTransportProtocol":true}}}'` (no desktop, `PKG_CONFIG_PATH` com o `alsa.pc` local). Copie `.AppImage` e `.sig`.
3. Prova Linux: `.verificacao/27-11-rust-tauri/ponte/prova-linux.sh` (modelo; ajuste `T` e `REPO`). Ela sobe Xvfb `:99`, `dbus-daemon` com config mínima, `notificacoes.py` (servidor de notificações falso que registra cada `Notify`), `metacity`, o release falso (`http.server`), semeia `chave.txt` falsa, vocabulário, `ajustes.json` com `notificacoes: false` e 2 linhas no histórico, e instala o Python pelo `instalar.sh` adaptado (API local, sem `pkill`, sem `pip`; dependências pelo `PYTHONUSERBASE` real).
   - Falha (AppImage com 1 byte trocado): log `ponte: falhou (integridade: o arquivo não confere com a assinatura); falha 1 seguida`, sem `ditado do Python pausado`, `.ponte-falhas` = 1, Python vivo e reagindo ao `xdotool keydown ctrl super` (`gravou 1.5s`), nenhum AppImage nem `RenanTalk.desktop` do Rust, pasta e autostart do Python intactos.
   - 3ª falha (reinicia o Python 2 vezes): contador 3, `aviso por notificação (3 falhas seguidas)` uma vez só, e o `Notify` chega ao D-Bus mesmo com notificações desligadas.
   - Sucesso (latest.json com o AppImage bom, Python reiniciado): log `assinatura confere` → `ditado do Python pausado` → `instalado em …/RenanTalk.AppImage` → `iniciado v0.0.910` → `pronto: gancho de teclado ligado (autostart ligado)` → `app novo confirmou` → `pasta do Python removida` → `Python encerrado`; `renantalk` rodando e reagindo ao Ctrl+Super; `~/.config/autostart/renantalk.desktop` sumiu e `RenanTalk.desktop` aponta para o AppImage; `~/.local/share/RenanTalk/app` removida; hashes de `chave.txt`, `vocabulario.txt`, `historico.db`, `ajustes.json` iguais; `~/.config/autostart` real intacto.
   - Interrupção (`prova-linux-2.sh`): depois do sucesso, recrie a pasta `app` e o autostart do Python, grave `.ponte-em-curso` e suba o `ditado.pyw` com o Rust rodando → `ponte: o app novo v… já roda (ponte interrompida antes de remover o Python)` → `pasta do Python removida` → `Python encerrado (o app novo já roda)`; `.ponte-em-curso` apagado.
4. Prova Windows: push na `rust-tauri` dispara `ponte.yml` (só teste, sem publicar). Ele prende um arquivo em `app\` para provar o caminho de reserva: log `sobrou parte da pasta do Python … (limpeza pid N, %TEMP%\RenanTalk-ponte-limpeza.log)` e, depois que o Python sai, `ponte: pasta do Python removida`. O autostart do Rust vai sempre para **HKCU**, com o caminho entre aspas (`casca::ligar_inicio` usa `reg add`, não o auto-launch, que tentaria HKLM num processo elevado). A ponte apaga o valor `RenanTalk` em HKCU e HKLM numa falha.
   - Armadilha já vista: PowerShell com `DETACHED_PROCESS` morre antes de rodar o roteiro (sem registro). Use `CREATE_NO_WINDOW` + grupo próprio.
5. Acompanhar o Windows: `gh run list --branch rust-tauri --workflow ponte.yml`, `gh run view <id> --log-failed`. O passo "Ponte (falha e sucesso)" imprime `OK`/`FALHA` por conferência e termina em `ponte no Windows: tudo conferido`.

## Não dá para provar aqui
- Windows com sessão interativa real (bandeja, toast visível, Windows Defender/SmartScreen no NSIS baixado, o processo destacado apagando `app\` com o usuário logado depois do boot): notebook `Betinhos-Renan`.
- Release real: o primeiro release da `main` com `latest.json` + zip da ponte; o Python antigo se atualiza para a ponte e só no ciclo seguinte migra.

## Evidências
- `.verificacao/<data>-<branch>/ponte/` (`prova-linux.out`, script e servidor de notificações).
