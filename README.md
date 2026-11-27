# RenanTalk

Ditado por voz para Windows, leve e direto ao ponto. Segure **Ctrl + Win**, fale e solte: o texto aparece onde estiver o cursor, em qualquer programa.

A transcrição usa o **Whisper large-v3 na Groq**, com ótima precisão em português e resposta em menos de 1 segundo. O programa ocupa cerca de **30 MB de RAM** e praticamente nada de CPU quando está parado.

## Como funciona

| Momento | O que você vê |
|---|---|
| Segurou Ctrl + Win | Bipe agudo e uma linha de luz azul nasce no centro da borda inferior da tela ativa (logo acima da barra de tarefas). O brilho acompanha a sua voz. |
| Soltou | Bipe e a luz pulsa do centro para as pontas enquanto transcreve. |
| Pronto | O texto é colado com Ctrl + V e a área de transferência volta ao que era antes. |
| Erro | Bipe grave e a luz apaga em vermelho. O motivo fica no log. |

- Apertar outra tecla junto (ex.: Ctrl + Win + →) cancela o ditado e deixa o atalho do Windows funcionar.
- Gravações muito curtas ou em silêncio são ignoradas (o Whisper costuma inventar texto quando recebe silêncio).

## Instalação

Requisitos: Windows 10/11, Python 3.10+ e uma chave gratuita da Groq ([console.groq.com/keys](https://console.groq.com/keys)).

```powershell
irm https://raw.githubusercontent.com/renanrmsantos14/RenanTalk/main/instalar.ps1 | iex
```

O instalador baixa o último release, instala as dependências, coloca o RenanTalk para iniciar com o Windows e abre o programa. Na primeira vez, ele pede a chave da Groq.

## Ícone perto do relógio

Clique no ícone do RenanTalk (na área de notificação ou na setinha `^`):

- **Configurações…**: abre a janela do RenanTalk (segue o tema claro/escuro do Windows):
  - **Vocabulário**: nomes, siglas e termos que o ditado deve escrever certo (ex.: nomes de clientes, `Dataverse`, `RD Station`). Adicione com Enter, remova no ✕; salva sozinho e vale no próximo ditado. Cabem cerca de 850 caracteres; o que passar fica riscado e de fora.
  - **Início e Substituições**: resumo dos ditados e sugestões de termos novos (aceite ou ignore); comandos falados ("nova linha", "ponto final", "vírgula"…) e regras próprias `quando eu disser => escreva` em `substituicoes.txt`.
  - **Chave Groq**: cola a chave nova; ela é validada na Groq antes de ser salva.
  - **Sobre**: versão, memória em uso, ditados do dia, atualização e log.
- **Procurar atualização**: verifica agora em vez de esperar o ciclo automático.

A janela de configurações roda num processo à parte (janela de aplicativo do Edge + servidor local em `127.0.0.1` com token). Ela usa cerca de 500 MB **só enquanto está aberta**; ao fechar, tudo encerra e o RenanTalk volta aos ~30 MB.
- **Ver log** e **Sair**.

## Atualização automática

Todo push na `main` que altera `app/` ou `requirements.txt` gera um release novo pelo GitHub Actions (versão `0.0.N`, PATCH + 1, com data e hora de Brasília).

O app instalado procura releases 1 minuto depois de iniciar e depois a cada 6 horas. Quando há versão nova, ele baixa o `RenanTalk.zip`, substitui os arquivos, instala dependências novas, avisa com uma notificação e reinicia sozinho, sem interromper um ditado em andamento. A versão atual aparece no menu do ícone.

## Onde ficam as coisas

| O quê | Onde |
|---|---|
| Programa | `%LOCALAPPDATA%\RenanTalk\app` |
| Chave da Groq | `%APPDATA%\RenanTalk\chave.txt` (nunca vai para o repositório) |
| Vocabulário | `%APPDATA%\RenanTalk\vocabulario.txt` (um termo por linha; `#` comenta) |
| Histórico de ditados | `%APPDATA%\RenanTalk\historico.db` (SQLite; textos dos últimos 30 dias, só neste computador) |
| Log | `%APPDATA%\RenanTalk\ditado.log` |
| Início automático | `shell:startup\RenanTalk.lnk` |

## Estrutura do código

```
app/
  ditado.pyw      atalho, gravação, envio à Groq, colagem e ciclo de atualização
  luz.py          linha de luz (janela em camadas, transparente e sem foco)
  bandeja.py      ícone, menu, notificações, chave e vocabulário
  configuracoes.py  janela de configurações (servidor local + Edge em modo app)
  ui.html         interface da janela de configurações
  atualizador.py  leitura do último release e troca dos arquivos
  VERSION         preenchido pelo workflow de release ("dev" localmente)
.github/workflows/release.yml
instalar.ps1
```

Para rodar do código-fonte: `py -3 -m pip install --user -r requirements.txt` e `pythonw app\ditado.pyw`. Com `VERSION` em modo `dev`, a atualização automática fica desligada.

## Privacidade

O áudio de cada ditado é enviado à API da Groq para transcrição. O áudio nunca é gravado em disco. O texto de cada ditado fica guardado só neste computador, em `historico.db`, por 30 dias (depois é apagado sozinho); você pode ver, corrigir, excluir um item ou apagar tudo na aba Histórico. O log registra só duração e tamanho do texto, nunca o conteúdo.

## Problemas comuns

- **Nada é colado:** veja o log pelo menu. Erro 401 significa chave inválida; troque pelo menu.
- **O menu Iniciar abre ao soltar o Win:** o RenanTalk já envia uma tecla neutra para evitar isso; se acontecer, avise com o log.
- **A luz não aparece:** ela só aparece enquanto você segura o atalho, no monitor da janela ativa.
