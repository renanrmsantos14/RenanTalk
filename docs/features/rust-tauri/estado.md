# RenanTalk desktop em Rust no modelo do Posta

- Nível: 2 (reescrita do app desktop inteiro: núcleo, instalador, atualização e tela; não cabe numa sessão)
- Etapa: 1 de 7 Alinhar
- Próximo passo: terminar o alinhamento (perguntas abertas abaixo), numa sessão nova · com você
- Branch e PR: rust-tauri · ainda não
- Protótipo: pendente (a tela em React segue o visual atual do `app/ui/index.html`?)
- Go: pendente
- PROD: ainda não
- Atualizado: 27/11/2026 15:04

## Referência
- Posta (`~/Desktop/Projetos/Posta`, repo `renanrmsantos14/posta`): núcleo em crate Rust (`crates/mail-core`), casca Tauri 2 com comandos (`apps/client/src-tauri`), tela React + Vite (`apps/client`, com dados de exemplo fora do Tauri), pnpm workspace, atualização assinada do Tauri, release por tag.
- Hoje no RenanTalk (cerca de 4.300 linhas): ditado Python (`app/*.py`, `app/ditado.pyw`), janela Rust/Tauri 2 (`config/src/main.rs`) com `app/ui/index.html` (12 abas), Android Kotlin separado (`android/`).

## Decisões
| Data | Decisão | Por quê | Quem decidiu |
|---|---|---|---|
| 27/11/2026 | Só desktop (Windows e Linux); o Android Kotlin continua como está | Menor escopo; o Android já funciona | Renan |
| 27/11/2026 | Tela em React, como no Posta (refazer as 12 abas) | Igual ao Posta | Renan |
| 27/11/2026 | Transição por versão-ponte automática: a última versão Python instala o app novo, migra ajustes, histórico e chave Groq e se desinstala | Ninguém reinstala à mão | Renan |
| 27/11/2026 | PRs #14 (reabrir a janela ao atualizar), #15 (colar sem deixar o ditado na área de transferência) e #16 (bandeira dos EUA na luz em inglês) mergeados; o comportamento deles vira cenário de aceite da versão Rust | Valem até a troca | Renan |

## Perguntas abertas (alinhamento)
- Linux: AppImage e/ou .deb (como o Posta) no lugar do zip + `instalar.sh`?
- Chave de assinatura da atualização do Tauri: gerar uma nova (secret `TAURI_SIGNING_PRIVATE_KEY`) ou reaproveitar a do Posta?
- Dados: manter `%APPDATA%\RenanTalk` / `~/.config/RenanTalk` com os mesmos arquivos (`ajustes.json`, `historico.db`, `vocabulario.txt`, `substituicoes.txt`, chave) para a ponte não precisar converter nada?
- Ordem das fatias: começar por um teste de 1 a 2 dias no Windows (atalho segurando Ctrl+Win por gancho de teclado de baixo nível, gravação, Groq, colar devolvendo a área de transferência) antes de refazer a tela?
- Luz sobreposta (janela em camadas no Windows, override-redirect no X11): manter a mesma aparência, incluindo a bandeira do #16?

## Riscos conhecidos
- Atalho só com modificadores (Ctrl+Win segurado) não é suportado pelos plugins de atalho global; exige gancho de teclado próprio (Windows `SetWindowsHookEx`, X11 RECORD).
- A versão-ponte é a única chance: se falhar, a máquina fica no Python sem caminho de atualização. Precisa de volta (manter o Python se a instalação nova falhar).
- Comportamentos sutis do Python a portar com teste: colar devolvendo todos os formatos (#15), abaixar o volume e app que fecha durante o ditado, reabrir a janela ao atualizar (#14), modos de IA por app, substituições, sugestões, retenção do histórico, conferência dos modelos da Groq.

## Tarefas e provas
| # | Tarefa | Depende de | Cenário (ação → resultado) | Prova | Nível | Resultado |
|---|---|---|---|---|---|---|

## Pendências e fora de escopo
- Android (fica Kotlin).

## Retro
-
