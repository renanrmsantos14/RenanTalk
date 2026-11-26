# RenanTalk Android — estado

- **Nível:** 2 (feature grande)
- **Etapa:** 2 de 6 — plano técnico proposto; aguardando o "Go" do Renan
- **Próximo passo:** Renan responde o card do Go (plano, layout do teclado, chave de assinatura).

## Decisões (26/11/2026)
- Formato: **teclado (IME) com botão de microfone** — segura, fala, solta; o texto entra no campo focado. Linha azul animada no teclado (mesma identidade da luz do desktop).
- Tecnologia: **Kotlin nativo** (Tauri mobile não faz IME).
- Transcrição: Groq `whisper-large-v3`, `language=pt`, vocabulário como `prompt` (mesma lógica de `app/ditado.pyw`), ignorar silêncio/toques curtos.
- Chave Groq: digitada no app, guardada só no aparelho (armazenamento criptografado).
- Vocabulário: **lista própria no celular**, independente dos PCs (decisão do Renan em 26/11/2026, depois de ver que sincronizar exigiria um token em cada aparelho). Vem com os mesmos termos padrão do desktop e é editada no app. Sincronização fica para depois, se fizer falta.
- Distribuição: APK assinado no mesmo release do GitHub (`RenanTalk.apk`); chave de assinatura como secret do repositório; app checa o último release e abre o instalador (Android exige confirmação do usuário).
- Teste: **celular do Renan** com depuração USB/sem fio (adb).
- CI: entra no `.github/workflows/release.yml` (PR compila; merge publica), sem quebrar os pacotes Windows/Linux.

## Ambiente (desktop renan-H410M-H, verificado em 26/11/2026)
- JDK 17.0.21 em `~/dev/jdk17` (fora do PATH; o build usa `JAVA_HOME` explícito).
- Android SDK em `~/Android/Sdk`: cmdline-tools, platform-tools 37, platforms 30–34, build-tools 30.0.3 e 34.0.0. Cópia duplicada em `~/.local/share/android-sdk` (não usar).
- adb 1.0.41 (37.0.3) em `~/.local/bin/adb`. Gradle 8.9 e 9.3.1 no cache. Nada exige sudo.
- Falta (instalar no usuário, sem sudo, depois do Go): `platforms;android-35` e `build-tools;35.0.0`.

## Plano técnico (proposto em 26/11/2026)
- **Projeto:** `android/`, um módulo `:app`, Kotlin + Views (sem Compose), Gradle wrapper, AGP 8.7, compileSdk/targetSdk 35, minSdk 26. Pacote `com.renantalk`. Sem dependências além de AndroidX core/appcompat e coroutines.
- **Arquivos:** `TecladoService.kt` (InputMethodService), `Gravador.kt` (AudioRecord 16 kHz mono PCM16, nível RMS para a luz, pico < 800 = silêncio, < 0,4 s = toque curto), `Groq.kt` (multipart com HttpURLConnection: `whisper-large-v3`, `language=pt`, `temperature=0`, `prompt`=vocabulário até 850 caracteres), `Cofre.kt` (chave AES-GCM no Android Keystore; só o texto cifrado fica em SharedPreferences), `Vocabulario.kt` (lista local com os 13 termos padrão do desktop), `LuzView.kt` (linha azul: nasce no centro, brilho pela voz, pulsa ao transcrever, vermelho no erro), `ConfigActivity.kt` (chave validada em `/openai/v1/models`, vocabulário, permissão do microfone, ativar teclado, sobre/versão, atualização), `Atualizador.kt`.
- **Permissões:** `RECORD_AUDIO` (pedida pela ConfigActivity; o teclado não consegue pedir), `INTERNET`, `REQUEST_INSTALL_PACKAGES` (atualização). Nenhuma outra.
- **Inserir texto:** `currentInputConnection.commitText(texto + " ", 1)`; vibração curta no lugar dos bipes. Em campo de senha o microfone fica desativado.
- **Versão:** a mesma do release (`0.0.N`); `versionCode` = maior·10000 + menor·100 + patch; data/hora de Brasília em `BuildConfig`, mostrada no canto do teclado e nas configurações.
- **Atualização:** a ConfigActivity (e uma checagem ao abrir o teclado, no máximo 1×/6 h) lê `releases/latest`; se a versão for maior, baixa `RenanTalk.apk` e abre o instalador (Android exige o toque do usuário e a permissão "instalar apps desconhecidos" na 1ª vez).
- **Assinatura:** keystore gerado com `keytool` nesta máquina; secrets `ANDROID_KEYSTORE_B64`, `ANDROID_KEYSTORE_SENHA`, `ANDROID_CHAVE_ALIAS`, `ANDROID_CHAVE_SENHA`. Backup do keystore fora do repo é obrigatório: sem ele, nenhum APK futuro instala por cima.
- **CI (`release.yml`):** `android/**` entra nos `paths`; job novo `android` (ubuntu, JDK 17, cache do Gradle) em paralelo ao `janela`. PR: `assembleDebug` + testes unitários, sem secrets. Push na main: `assembleRelease` assinado com a versão do job `versao`; o job `release` passa a depender dele e anexa `RenanTalk.apk`. Windows/Linux não mudam.
- **Testes:** unitários para a decisão silêncio/toque curto, montagem do WAV e corte do vocabulário; prova final no celular do Renan via adb (vídeo da tela).

## Riscos conhecidos
- Release com mudança só no Android gera versão nova e os PCs se "atualizam" para o mesmo código (um reinício inofensivo). Aceito para manter uma versão única.
- O teclado só com microfone obriga a trocar de teclado para digitar; mitigado com o botão de trocar teclado (globo) e apagar/espaço/enter.

## Contexto do projeto
- Repo: https://github.com/renanrmsantos14/RenanTalk (público). Windows/Linux v0.0.7 em produção nos 3 PCs.
- Ramos abertos: `janela-instantanea` (janela aparece mais cedo + ditado.json; sem PR) e PR #3 (reinício não prende terminal).

## Tarefas
- [x] Plano técnico (módulos, permissões, layout do teclado, vocabulário local)
- [ ] Go do Renan
- [ ] Projeto Android (Gradle/Kotlin) em `android/`
- [ ] Teclado + gravação + Groq + inserir texto
- [ ] Tela de configurações (chave, vocabulário) e atualização pelo release
- [ ] CI com assinatura e APK no release
- [ ] Prova no celular do Renan (vídeo/prints) e PR

## Provas
(nenhuma ainda)
