# wt-monitor → Home Assistant → Alexa

> 🇺🇸 [Read in English](README.md)

Faça um Echo anunciar (ou mostrar na tela de um Echo Show) quando um agente precisar de você: uma pergunta ou um pedido de permissão sem resposta (`ask`). Os outros eventos (`done`, `mem`, `sysmem`, ...) também funcionam; escolha com `WT_NOTIFY_EVENTS`. Os eventos `ask` exigem os hooks do agente (`make hooks`).

```
wt-monitor ──WT_NOTIFY_CMD──► wt-notify-ha.sh ──POST──► webhook do Home Assistant ──► Alexa Media Player ──► Amazon ──► Echo
```

## O que sai da sua máquina

O wt-monitor sozinho não envia nada. O webhook fica na sua rede (`local_only: true`). **A frase que o Echo fala ou mostra passa pela nuvem da Amazon**: não existe jeito local de fazer um Echo falar. A automação de exemplo manda só uma frase curta com o nome do worktree e o tipo do evento; a mensagem do agente fica no Home Assistant.

O Alexa Media Player é uma **integração da comunidade, sem suporte da Amazon nem do Home Assistant**. Ele entra na sua conta e guarda a sessão da Amazon em `config/.storage` do Home Assistant. Mantenha essa pasta fora de git e de backups sem criptografia, desligue as compras por voz no app Alexa e não exponha o Home Assistant à internet. Informar a chave do app 2FA deixa a integração mais estável, mas quem ler essa pasta passa a ter os dois fatores da sua conta Amazon.

## 1. Home Assistant

Pule se você já tem um. Na mesma máquina, com Docker:

```yaml
# ~/homeassistant/compose.yaml   →   docker compose up -d
services:
  homeassistant:
    container_name: homeassistant
    image: ghcr.io/home-assistant/home-assistant:stable
    volumes:
      - ./config:/config
      - /etc/localtime:/etc/localtime:ro
    network_mode: host
    restart: unless-stopped
```

Abra `http://<esta máquina>:8123` e crie seu usuário.

## 2. Alexa Media Player

1. **Instale.** Home Assistant 2026.11+: *Configurações → Marketplace*, aceite o aviso, procure **Alexa Media Player** (ou ⋮ → *Add from link* → `alandtse/alexa_media_player`) e instale. Versões anteriores: instale pelo HACS.
2. **Adicione.** *Configurações → Dispositivos e serviços → Adicionar integração → Alexa Media Player*, com e-mail e senha da Amazon, o domínio da sua conta (ex.: `amazon.com.br`) e uma URL do Home Assistant que o seu navegador abra. A chave do app 2FA é opcional (veja acima).
3. **Encontre seu Echo.** Os Echos aparecem como entidades `media_player.<nome>` (*Configurações → Entidades*).

## 3. Automação

Importe [`automation.yaml`](automation.yaml) (*Configurações → Automações → ⋮ → Editar em YAML*) e troque:

- `webhook_id`: um valor longo e aleatório (`openssl rand -hex 16`). Ele funciona como senha.
- `target`: o seu Echo, ex.: `media_player.echo_show`. Uma lista envia para vários.
- `method`: `all` (voz + tela), `speak` (só voz) ou `show` (só na tela, num Echo Show).
- O horário em `conditions`, se quiser.

## 4. wt-monitor

```bash
install -m755 contrib/home-assistant/wt-notify-ha.sh ~/.local/bin/
( umask 077; echo 'http://127.0.0.1:8123/api/webhook/<seu-webhook-id>' > ~/.config/wt-monitor/ha-webhook )
systemctl --user edit wt-monitor
```

No editor:

```ini
[Service]
Environment=WT_NOTIFY_CMD=%h/.local/bin/wt-notify-ha.sh
Environment=WT_NOTIFY_EVENTS=ask
```

Depois `systemctl --user restart wt-monitor` (isso fecha os terminais abertos pelo painel e volta as prioridades ao normal). Use `127.0.0.1` se o Home Assistant roda na mesma máquina; senão, o endereço dele na sua rede.

## 5. Teste

```bash
WT_NOTIFY_JSON='{"event":"done","worktree":"teste","title":"teste","body":"teste","urgent":false,"ts":0}' ~/.local/bin/wt-notify-ha.sh
```

O Home Assistant responde ao webhook com sucesso mesmo quando a automação falha. Se nada acontecer, veja o log (*Configurações → Sistema → Logs*). `Action notify.alexa_media not found` quer dizer que o Alexa Media Player não carregou (às vezes ele esgota o tempo depois de um reinício): recarregue-o em *Dispositivos e serviços*.
