# RenanTalk Android — estado

- **Nível:** 2 (feature grande)
- **Etapa:** 3 de 6 — realinhado: bolha flutuante (igual Wispr Flow) no lugar do teclado; protótipo no POCO
- **Próximo passo:** protótipo da bolha (acessibilidade + gravar + inserir no WhatsApp) provado no POCO X5 Pro (HyperOS, Android 14).

## Mudança de direção (26/11/2026, decisão do Renan)
- O RenanTalk **não é mais um teclado**: funciona como o Wispr Flow no Android — uma **bolha flutuante** que aparece acima do teclado que já estiver em uso (Gboard) quando ele abre e some quando fecha. O texto entra no campo focado pelo **serviço de acessibilidade**.
- **Gestos:** os dois, igual Wispr — segurar e falar (soltar envia) **ou** tocar para começar e tocar de novo para parar.
- **Teclado removido** (QWERTY, acentos, painel, Personalizar teclado). Continuam: Gravador, Groq, Cofre, Vocabulário, Atualizador, ConfigActivity e a linha de luz do PC (OndaView).
- **Protótipo primeiro** no POCO, antes de reescrever o app: riscos a provar são o HyperOS matar o serviço/bolha, o Android 13+ bloquear acessibilidade de app instalado fora da loja ("configurações restritas") e o Android 14 barrar o microfone com o app em segundo plano.
- Histórico do teclado (decisões e provas abaixo) fica como registro; o código sai do ramo.
- Provas que já valem para a bolha: painel de voz/linha de luz e retorno do microfone (escala em dB, aviso "Não estou ouvindo").

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

## Respostas do card (26/11/2026)
- Layout: **faixa compacta** (globo, apagar, microfone grande, espaço, enter, linha azul em cima, versão no canto).
- Assinatura: **a IA faz tudo** (gera keystore, cadastra os secrets via `gh`, entrega o arquivo para backup).
- Go: ainda não; Renan pediu para ver antes se existe teclado open source para partir dele.
- Base: **código próprio (Go)** em 26/11/2026, depois de ver a avaliação abaixo.

## Teclados open source avaliados (26/11/2026, via GitHub API)
| Projeto | Licença | Tamanho | Encaixe |
|---|---|---|---|
| DevEmperor/DictateKeyboard (v6.5.0, ativo) | Apache-2.0 | ~100 MB de repo, teclado completo | Já tem Groq, segurar para falar, Whisper. Grande demais para fork enxuto; bom para **usar pronto** ou como referência de código (Apache permite copiar com atribuição). |
| j3soon/whisper-to-input | GPL-3.0 | ~60 KB de Kotlin | Layout quase igual ao nosso, endpoint configurável; mas é tocar-para-ligar, sem prompt/silêncio/Keystore/atualização, e obriga GPL-3 no `android/`. |
| florisboard, HeliBoard | Apache-2.0 / GPL-3.0 | teclados de digitação completos | Sem ditado por API; fork pesado. |

## Riscos conhecidos
- Release com mudança só no Android gera versão nova e os PCs se "atualizam" para o mesmo código (um reinício inofensivo). Aceito para manter uma versão única.
- O teclado só com microfone obriga a trocar de teclado para digitar; mitigado com o botão de trocar teclado (globo) e apagar/espaço/enter.

## Contexto do projeto
- Repo: https://github.com/renanrmsantos14/RenanTalk (público). Windows/Linux v0.0.7 em produção nos 3 PCs.
- Ramos abertos: `janela-instantanea` (janela aparece mais cedo + ditado.json; sem PR) e PR #3 (reinício não prende terminal).

## Tarefas
- [x] Plano técnico (módulos, permissões, layout do teclado, vocabulário local)
- [x] Go do Renan (código próprio)
- [x] Projeto Android (Gradle 8.9, AGP 8.7.3, Kotlin 2.0.21, sem dependências de runtime) em `android/`
- [x] Teclado + gravação + Groq + inserir texto
- [x] Tela de configurações (passos, chave, vocabulário, teste) e atualização pelo release (PackageInstaller)
- [x] CI com assinatura e APK no release (job `android`; release não depende do APK)
- [x] Keystore em `~/.config/RenanTalk-android/` (desktop) + 4 secrets no GitHub — **Renan precisa guardar cópia no gerenciador de senhas**
- [ ] Prova no celular do Renan (vídeo/prints) e PR

## Provas
- 26/11/2026: `testDebugUnitTest` 10/10 passando (silêncio, toque curto, prompt igual ao desktop, WAV, versões, nível da luz).
- 26/11/2026: `assembleRelease` assinado (54 KB), `apksigner verify` ok, cert SHA-256 `8c5590e2…e42326`. Lint sem erros (3 avisos aceitos: allowBackup, path longo do globo, UnsafeIntentLaunch mitigado por código aleatório).
- Falta: prova no celular (vídeo/prints) e CI verde no PR.
