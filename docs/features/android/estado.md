# RenanTalk Android — estado

- **Nível:** 2 (feature grande)
- **Etapa:** 1 de 6 — alinhamento concluído; próximo: plano técnico + "Go"
- **Próximo passo:** sessão nova lê este arquivo, propõe o plano técnico e pede o Go num card.

## Decisões (26/11/2026)
- Formato: **teclado (IME) com botão de microfone** — segura, fala, solta; o texto entra no campo focado. Linha azul animada no teclado (mesma identidade da luz do desktop).
- Tecnologia: **Kotlin nativo** (Tauri mobile não faz IME).
- Transcrição: Groq `whisper-large-v3`, `language=pt`, vocabulário como `prompt` (mesma lógica de `app/ditado.pyw`), ignorar silêncio/toques curtos.
- Chave Groq: digitada no app, guardada só no aparelho (armazenamento criptografado).
- Vocabulário: **sincronizar com os PCs** (decisão do Renan). Ponto em aberto: onde guardar a lista comum (ex.: Gist privado exige token GitHub em cada aparelho; avaliar alternativa sem novo segredo antes do Go).
- Distribuição: APK assinado no mesmo release do GitHub (`RenanTalk.apk`); chave de assinatura como secret do repositório; app checa o último release e abre o instalador (Android exige confirmação do usuário).
- Teste: **celular do Renan** com depuração USB/sem fio (adb).
- CI: entra no `.github/workflows/release.yml` (PR compila; merge publica), sem quebrar os pacotes Windows/Linux.

## Contexto do projeto
- Repo: https://github.com/renanrmsantos14/RenanTalk (público). Windows/Linux v0.0.7 em produção nos 3 PCs.
- Ramos abertos: `janela-instantanea` (janela aparece mais cedo + ditado.json; sem PR) e PR #3 (reinício não prende terminal).

## Tarefas
- [ ] Plano técnico (módulos, permissões, layout do teclado, sincronização do vocabulário) + Go
- [ ] Projeto Android (Gradle/Kotlin) em `android/`
- [ ] Teclado + gravação + Groq + inserir texto
- [ ] Tela de configurações (chave, vocabulário) e atualização pelo release
- [ ] CI com assinatura e APK no release
- [ ] Prova no celular do Renan (vídeo/prints) e PR

## Provas
(nenhuma ainda)
