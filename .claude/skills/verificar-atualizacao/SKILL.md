---
name: verificar-atualizacao
description: Verifica a atualização assinada do app em Rust (tauri-plugin-updater, latest.json, reabrir a janela) e os jobs desktop do release.yml. Use ao mudar apps/desktop/src-tauri/src/atualizar.rs, plugins.updater ou bundle do tauri.conf.json, bump-version.mjs ou release.yml.
---

# Verificar a atualização (apps/desktop)

1. Nunca use a chave real (`~/.config/RenanTalk-desktop-assinatura/`). Gere uma de teste: `pnpm --filter desktop exec tauri signer generate --ci -w <pasta>/teste.key -p ""`.
2. AppImages N e N+1 com a pubkey de teste e `http` liberado (só no build de teste): `RENANTALK_VERSAO=0.0.900 node apps/desktop/scripts/bump-version.mjs`, depois em `apps/desktop` `TAURI_SIGNING_PRIVATE_KEY="$(cat teste.key)" TAURI_SIGNING_PRIVATE_KEY_PASSWORD="" pnpm tauri build --bundles appimage --config '{"plugins":{"updater":{"pubkey":"<teste.key.pub>","dangerousInsecureTransportProtocol":true}}}'`. Repita com 0.0.901. Depois volte `package.json`, `version.json`, `tauri.conf.json` e `Cargo.toml` à versão do repo. Sem libasound2-dev: `PKG_CONFIG_PATH` com um `alsa.pc` local.
3. Sirva `latest.json` (`version`, `platforms.linux-x86_64.signature` = conteúdo do `.sig`, `url`) e o AppImage N+1 com `python3 -m http.server 38901 --bind 127.0.0.1`.
4. Rode o N no Xvfb `:99` (nunca a tela real) dentro de `dbus-run-session`, com `HOME`/`XDG_CONFIG_HOME` de teste e `RENANTALK_UPDATE_URL=http://127.0.0.1:38901/latest.json`. A 1ª subida numa sessão D-Bus nova leva ~45 s (portais); espere `iniciado` no `ditado.log` antes de rodar `--aba=sobre`.
5. Clique em "Procurar agora" (`xdotool mousemove --window <janela 800x580> 687 221 click 1`; se a página ainda carregava, clique de novo). Esperado no log: `nova v0.0.901`, `instalando v0.0.901 (janela sob)`, `iniciado v0.0.901`, `instalada (v0.0.900 → v0.0.901)`; o AppImage foi trocado no lugar e a janela reabre na aba Sobre com 0.0.901. A verificação automática roda 60 s depois de subir.
6. Adulterado: sirva o AppImage N+1 com um byte a mais e o `.sig` original → log `erro (a assinatura de v0.0.901 não confere; nada foi instalado …)`, AppImage instalado inalterado, sem `.atualizando` na pasta de dados.
7. CI: `actionlint` em `.github/workflows/`; o job `pacotes` do `rust.yml` roda o mesmo build do modo PR do `release.yml` (NSIS e AppImage sem assinar).
   - `release.yml`: o job `release` só publica com `github.ref == 'refs/heads/main'`. O passo do `latest.json` confere cada `.sig` com a pubkey do `tauri.conf.json` (`app/minisign.py`, exigindo `file:<arquivo>` no comentário); assinatura que não confere vira `::error::` e o `latest.json` só sai com as 2 plataformas. Prova local: rode o trecho Python do passo com um `.sig` de teste e um trocado.
   - Atualização adiada (ditado em curso): tenta de novo 60 s depois, não 6 h; o volume é restaurado (`d.encerrar()`) antes do `install`.
8. Armadilhas: `pkill -f` mata o próprio shell (use `pgrep -x renantalk`); o `bump-version.mjs` reescreve 4 arquivos do repo.
