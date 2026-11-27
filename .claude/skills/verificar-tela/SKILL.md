---
name: verificar-tela
description: Verifica a tela React da janela de configurações em Rust/Tauri (apps/desktop). Use ao mudar apps/desktop/src, src-tauri/src/tela.rs, capabilities ou comandos de tela do talk-core.
---

# Verificar a tela React (apps/desktop)

1. Testes e build: `source <cargoenv>` (se faltar libasound2-dev, ver `pkg-config alsa`), `cargo test --workspace` e `pnpm --filter desktop build` (sobe o PATCH em `src/version.json`).
2. Dev server com mock (porta 36495, `~/.codex/dev-ports.json`): `pnpm --filter desktop dev`; Playwright headless nas 9 abas: busca/edição/exclusão (2 cliques) no Histórico, termo, regra e app, volume, atalho com menos de 2 teclas (erro local), tema escuro, chave com "falsa" mostra "A Groq recusou essa chave.", console sem erro.
3. Dentro do Tauri (Xvfb `:99`, nunca a tela real; HOME e XDG_CONFIG_HOME numa pasta de teste): depois de `pnpm build`, `touch src-tauri/src/main.rs` e `cargo build -p renantalk --release`. Abra o `renantalk` residente, espere 3 s e rode `renantalk --aba=<aba>` (a 2ª execução abre a janela); capture com `import -window <id 800x580>` (o `xdotool search --name '^RenanTalk$' | tail -1`). Compare com `docs/features/rust-tauri/prints/antes-<aba>.png`.
4. Efeito: editar o Histórico muda `historico.db` (tabela `ditado`, editado=1, `original` intacto); mudar o volume grava `ajustes.json`; chave falsa mostra o erro e não cria `chave.txt` nem aparece no `ditado.log`.
5. Armadilhas: se o Xvfb parar de responder (`xdpyinfo` trava), reinicie-o; nunca rode dois scripts de captura ao mesmo tempo; `pkill -f` mata o próprio shell.
