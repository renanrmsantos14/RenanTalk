# RenanTalk desktop em Rust no modelo do Posta

- Nível: 2 (reescrita do app desktop inteiro: núcleo, instalador, atualização e tela; não cabe numa sessão)
- Etapa: 5 de 7 Construir
- Próximo passo: tarefa 1 (workspace + talk-core ajustes), numa sessão nova · sozinho
- Branch e PR: rust-tauri · ainda não
- Protótipo: sem protótipo (a tela copia o visual atual do `app/ui/index.html`)
- Spec: `docs/features/rust-tauri/spec.md`
- Go: 27/11/2026 15:14 por Renan (clique no card)
- PROD: ainda não
- Atualizado: 27/11/2026 15:14

## Referência
- Posta (`~/Desktop/Projetos/Posta`, repo `renanrmsantos14/posta`): núcleo em crate Rust (`crates/mail-core`), casca Tauri 2 com comandos (`apps/client/src-tauri`), tela React + Vite (`apps/client`, com dados de exemplo fora do Tauri), pnpm workspace, atualização assinada do Tauri, release por tag.
- Hoje no RenanTalk (cerca de 4.300 linhas): ditado Python (`app/*.py`, `app/ditado.pyw`), janela Rust/Tauri 2 (`config/src/main.rs`) com `app/ui/index.html` (9 abas no menu; o "12 abas" anterior estava errado), Android Kotlin separado (`android/`).

## Decisões
| Data | Decisão | Por quê | Quem decidiu |
|---|---|---|---|
| 27/11/2026 | Só desktop (Windows e Linux); o Android Kotlin continua como está | Menor escopo; o Android já funciona | Renan |
| 27/11/2026 | Tela em React, como no Posta (refazer as 12 abas) | Igual ao Posta | Renan |
| 27/11/2026 | Transição por versão-ponte automática: a última versão Python instala o app novo, migra ajustes, histórico e chave Groq e se desinstala | Ninguém reinstala à mão | Renan |
| 27/11/2026 | PRs #14 (reabrir a janela ao atualizar), #15 (colar sem deixar o ditado na área de transferência) e #16 (bandeira dos EUA na luz em inglês) mergeados; o comportamento deles vira cenário de aceite da versão Rust | Valem até a troca | Renan |
| 27/11/2026 | Linux só em AppImage (sem .deb, sem zip + `instalar.sh`) | Sem sudo; a atualização do Tauri troca o AppImage sozinha | Renan |
| 27/11/2026 | Chave de assinatura nova, só do RenanTalk | Um vazamento não afeta o Posta | Renan |
| 27/11/2026 | Chave Groq continua em `chave.txt`; mesma pasta e mesmos arquivos de dados | A ponte não converte nada; sem depender de cofre no Linux | Renan |
| 27/11/2026 | Processo único com a janela sob demanda (criada ao abrir, destruída ao fechar) | Parado, gastar perto do Python (~22 MB), e não ~200 MB | Renan |
| 27/11/2026 | Sem teste de viabilidade separado; a 1ª fatia já é o ditado de ponta a ponta | Renan: "não precisa" | Renan |
| 27/11/2026 | Luz igual à de hoje (cores, espessura, posição e bandeira) | A luz atual é o critério de aceite | Renan |
| 27/11/2026 | Tela React copia o visual atual, sem protótipo | Prints de hoje são o "antes" | Renan |
| 27/11/2026 | Ponte que falha: o Python segue, tenta de novo a cada verificação (6 h) e avisa na 3ª falha | Ninguém fica sem ditado | Renan |
| 27/11/2026 | Assumi: NSIS por usuário sem admin; autostart pelo plugin; bandeja do Tauri; um release só com instaladores, `latest.json` e APK; numeração `v0.0.N` | Padrão do Posta e o Android lê `releases/latest` | Claude (sem objeção) |

## Perguntas abertas (alinhamento)
- Nenhuma.

## Riscos conhecidos
- Atalho só com modificadores (Ctrl+Win segurado) não é suportado pelos plugins de atalho global; exige gancho de teclado próprio (Windows `SetWindowsHookEx`, X11 RECORD).
- A versão-ponte é a única chance: se falhar, a máquina fica no Python sem caminho de atualização. Precisa de volta (manter o Python se a instalação nova falhar).
- AppImage precisa de `libfuse2`: há no desktop; na Lenovo, não verifiquei (a prova da tarefa 8 confere).
- NSIS instala em `%LOCALAPPDATA%\RenanTalk`, pasta-mãe do Python: a ponte apaga só `app\` e o desinstalador não pode levar a pasta toda.
- Mutex "RenanTalk" do Python × instância única do Tauri: a ponte encerra o Python antes da confirmação do app novo.
- Comportamentos sutis do Python a portar com teste: colar devolvendo todos os formatos (#15), abaixar o volume e app que fecha durante o ditado, reabrir a janela ao atualizar (#14), modos de IA por app, substituições, sugestões, retenção do histórico, conferência dos modelos da Groq.

## Tarefas e provas
| # | Tarefa | Depende de | Cenário (ação → resultado) | Prova | Nível | Resultado |
|---|---|---|---|---|---|---|
| 1 | Workspace (pnpm + Cargo) e crate `talk-core` com ajustes: ler, normalizar e salvar `ajustes.json` compatível com o Python; testes de `test_ajustes.py` portados. Sonnet | — | `ajustes.json` gravado pelo Python → o Rust lê igual, e o Python relê o que o Rust gravou | `cargo test` com cópia real de `ajustes.json`, mais um script Python relendo | 4 | |
| 2 | `talk-core`: histórico (mesmo schema SQLite e retenção), vocabulário, substituições, comandos falados, sugestões e escrita com IA (modos e `modos_apps`); testes portados. Sonnet | 1 | Cenários 5 e 6 na lógica | `cargo test` com cópia de `historico.db`; saída colada | 4 | |
| 3 | Ditado de ponta a ponta no Linux X11: gancho Ctrl+Win/Shift+Win/Alt+Win+V (RECORD), gravação, Groq, colar devolvendo a área de transferência, bipe, abaixar volume e log. Sonnet (Opus se falhar 2×) | 2 | Cenários 1 a 4 e 13 | Xvfb `:99`: `xdotool` segura o atalho, áudio injetado por arquivo de teste, texto colado num campo, `xclip` mostra a área de transferência restaurada, log sem a chave | 4 | |
| 4 | Ditado no Windows: gancho `SetWindowsHookEx`, WASAPI, `SendInput`, área de transferência em todos os formatos, volume por sessão. Sonnet | 2 | Cenários 1 a 4 no Windows | Notebook `Betinhos-Renan` por `delegate_task`, cópia de teste com mutex renomeado; saída e print | 5 | |
| 5 | Luz sobreposta (janela em camadas no Windows, override-redirect no X11) com cores, espessura, posição e bandeira. Sonnet | 3, 4 | Cenários 2 e 4 (luz) | Print no Xvfb `:99` ao lado do print da luz Python (`prints/antes-luz*.png`); print no Windows | 4/5 | |
| 6 | Casca residente: bandeja, instância única, autostart, janela sob demanda, medição de memória. Sonnet | 3 | Cenários 8 e 9 | Xvfb `:99`: RSS parado antes e depois de abrir e fechar a janela; 2ª execução foca a janela; RSS no Windows | 4/5 | |
| 7 | Tela React, parte 1: Início, Histórico, Vocabulário e Substituições, com comandos Tauri sobre `talk-core` e dados de exemplo fora do Tauri. Sonnet | 2, 6 | Cenários 6 e 7 | `prints/antes-*.png` da janela atual; dev server com mock, interação e prints lado a lado; depois dentro do Tauri no Xvfb | 4 | |
| 8 | Tela React, parte 2: Escrita com IA, Ditado e som, Aparência, Chave Groq e Sobre (com versão de build). Sonnet | 7 | Cenários 7 e 13 | Mesmo caminho da 7; salvar ajuste → próximo ditado no Xvfb usa o valor novo | 4 | |
| 9 | Atualização assinada e release: chave nova (secrets no repo), `tauri-plugin-updater`, `release.yml` com tauri-action (NSIS + AppImage) e o APK no mesmo release, reabrir a janela ao atualizar. Opus (segurança) | 6 | Cenário 10 | `latest.json` de teste servido local, assinado, N → N+1 no Xvfb com a janela reabrindo; CI do PR compilando os dois SO; `libfuse2` conferido na Lenovo | 4 | |
| 10 | Versão-ponte no Python: baixa o instalador do release, confere a assinatura, instala, inicia o app novo, espera a confirmação e remove o Python; em falha, segue e tenta de novo, avisando na 3ª. Opus (sem volta) | 9 | Cenários 11 e 12 | Linux: Python instalado numa pasta de teste → ponte → app novo com os mesmos dados; falha forçada → Python segue. Windows: o mesmo na cópia de teste do notebook | 4/5 | |
| 11 | Limpeza depois da migração das 3 máquinas: tirar `app/`, `config/`, o zip do release e os instaladores antigos; atualizar README, `verificar-ditado` e `verificar-janela`. Sonnet | 10 e máquinas migradas | Release sem zip; skills passam no app novo | `verificar-ditado` e `verificar-janela` rodando no Xvfb | 4 | |

## Pendências e fora de escopo
- Android (fica Kotlin).
- Wayland, macOS, .deb e redesenho da tela.
- Entregas em PRs empilhados para a `main`: núcleo (1 a 6), tela (7 e 8), atualização e ponte (9 e 10), limpeza (11). A ponte só vai para a `main` com 1 a 9 provados.

## Retro
-
