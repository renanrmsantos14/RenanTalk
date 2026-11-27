# Spec: RenanTalk desktop em Rust no modelo do Posta

## Objetivo
Trocar o ditado Python e a janela Tauri separada (`app/`, `config/`) por um app Rust só, como o Posta: o núcleo fica na crate `crates/talk-core`, a casca Tauri 2 com a tela React + Vite em `apps/desktop`, tudo num pnpm workspace com atualização assinada do Tauri. As máquinas atuais migram sozinhas por uma versão-ponte do Python.

## Fora de escopo
- Android (`android/` continua em Kotlin; só o APK segue no mesmo release).
- Redesenho da tela: as 9 abas atuais (Início, Histórico, Vocabulário, Substituições, Escrita com IA, Ditado e som, Aparência, Chave Groq, Sobre) são copiadas com o visual de hoje.
- Wayland, macOS e .deb.

## Decisões técnicas (ver Decisões no estado)
- Linux: só AppImage. Windows: NSIS por usuário, sem admin. A atualização vem do `latest.json` assinado com uma chave nova (secrets `TAURI_SIGNING_PRIVATE_KEY` e `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`).
- Dados na mesma pasta de hoje (`%APPDATA%\RenanTalk`, `~/.config/RenanTalk`) e nos mesmos arquivos: `ajustes.json`, `historico.db`, `vocabulario.txt`, `substituicoes.txt`, `chave.txt` e `ditado.log`. Ler e gravar os mesmos formatos; o Python e o Rust têm de abrir os arquivos um do outro.
- Processo único: o núcleo, a bandeja e a luz ficam residentes. A janela (webview) é criada ao abrir e destruída ao fechar.
- Modelos da Groq: `whisper-large-v3` para o ditado e `openai/gpt-oss-120b` para a escrita, conferidos ao iniciar como hoje.
- Um release por versão `v0.0.N`, com o instalador Windows, o AppImage, o `latest.json` e o `RenanTalk.apk`. O Android lê `releases/latest` atrás do APK.

## Cenários de aceite (ação → resultado)
1. Segurar Ctrl+Win, falar e soltar → o texto transcrito é colado no campo em foco, e a área de transferência volta ao que era, em todos os formatos (#15).
2. Shift+Win → ditado em inglês, com a bandeira dos EUA na luz (#16).
3. Alt+Win+V → cola o último ditado.
4. Ditar → a luz aparece com a cor, a espessura e a posição dos ajustes; o bipe toca no volume configurado; com "abaixar outros" ligado, o volume dos outros apps cai e depois volta, mesmo que o app feche durante o ditado.
5. Ditar no Outlook, no WhatsApp ou no Code → o modo de IA daquele app é aplicado (`modos_apps`); as substituições e os comandos falados ("nova linha", "ponto final"…) são aplicados; os termos novos viram sugestões.
6. Cada ditado entra no histórico, com a retenção de hoje. Buscar, editar, excluir e apagar tudo funcionam na aba Histórico.
7. Abrir pela bandeja → as 9 abas com o visual atual; os prints de `prints/antes-*.png` são a referência. Salvar ajustes → o próximo ditado já usa o valor novo.
8. Ficar parado sem a janela aberta → memória residente medida nos dois sistemas e registrada. Meta (estimativa): até 60 MB.
9. Abrir o app de novo com ele já rodando → traz a janela existente (instância única). Ao ligar o computador, ele inicia sozinho.
10. Publicar a versão N+1 → o app N baixa, confere a assinatura, instala e reinicia; se a janela estava aberta, ela reabre (#14).
11. Máquina com o Python atual → em até 6 h recebe a versão-ponte, que instala o app novo e o inicia. O app novo confirma que subiu, e então a ponte remove o Python (pasta `app`, `Startup\RenanTalk.lnk` ou o autostart do Linux). Ajustes, histórico, vocabulário e chave continuam iguais.
12. A ponte falha (download, assinatura ou instalação) → o Python continua ditando, registra no log e tenta de novo na próxima verificação; na 3ª falha, avisa por notificação.
13. A chave Groq nunca aparece em log, histórico ou tela (só mascarada).

## O que muda e quem depende
- `app/atualizador.py` e o `release.yml` deixam de publicar `RenanTalk.zip` só depois de as 3 máquinas confirmarem a migração (última tarefa).
- `app/ui/index.html` e `config/` são substituídos; as skills `verificar-ditado` e `verificar-janela` são atualizadas para o app novo.
- Instalação no Windows: o NSIS instala em `%LOCALAPPDATA%\RenanTalk`, que é a pasta-mãe do Python (`...\RenanTalk\app`). A ponte apaga só `app\`, e o desinstalador do NSIS não pode apagar a pasta inteira.
- Mutex "RenanTalk" do Python e instância única do Tauri: a ponte encerra o Python antes de o app novo confirmar que subiu, ou elas se bloqueiam.

## Irreversível
- A versão-ponte: depois de remover o Python, voltar só com uma reinstalação manual. Por isso a remoção só acontece depois de o app novo confirmar que subiu (cenário 11).
- A chave de assinatura nova: perdida a chave, nenhuma atualização instala por cima. Cópia no gerenciador de senhas, como o keystore do Android.
