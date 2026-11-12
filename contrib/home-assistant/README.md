# wt-monitor → Home Assistant → Alexa

> 🇧🇷 [Leia em português](README.pt-BR.md)

Have an Echo announce (or show on an Echo Show's screen) when an agent needs you: a question or a permission prompt left unanswered (`ask`). Other events (`done`, `mem`, `sysmem`, ...) work too; pick them with `WT_NOTIFY_EVENTS`. The `ask` events need the agent hooks (`make hooks`).

```
wt-monitor ──WT_NOTIFY_CMD──► wt-notify-ha.sh ──POST──► Home Assistant webhook ──► Alexa Media Player ──► Amazon ──► Echo
```

## What leaves your machine

wt-monitor itself sends nothing. The webhook stays on your network (`local_only: true`). **The phrase the Echo says or shows goes through Amazon's cloud**: there is no local way to make an Echo speak. The example automation only sends a short phrase with the worktree's name and the event type; the agent's message stays in Home Assistant.

Alexa Media Player is a **community integration, not supported by Amazon or Home Assistant**. It logs in as you and stores your Amazon session in Home Assistant's `config/.storage`. Keep that folder out of git and unencrypted backups, turn off voice purchasing in the Alexa app, and do not expose Home Assistant to the internet. Giving it your 2FA app key makes it more reliable, but whoever reads that folder then has both factors of your Amazon account.

## 1. Home Assistant

Skip this if you already have one. On the same machine, with Docker:

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

Open `http://<this machine>:8123` and create your user.

## 2. Alexa Media Player

1. **Install it.** Home Assistant 2026.11+: *Settings → Marketplace*, accept the warning, search for **Alexa Media Player** (or ⋮ → *Add from link* → `alandtse/alexa_media_player`) and install. Older versions: install it through HACS.
2. **Add it.** *Settings → Devices & services → Add integration → Alexa Media Player*, with your Amazon e-mail and password, your Amazon domain (e.g. `amazon.com`, `amazon.com.br`) and a Home Assistant URL your browser can open. The 2FA app key is optional (see above).
3. **Find your Echo.** Your Echos show up as `media_player.<name>` entities (*Settings → Entities*).

## 3. Automation

Import [`automation.yaml`](automation.yaml) (*Settings → Automations → ⋮ → Edit in YAML*) and change:

- `webhook_id`: a long random value (`openssl rand -hex 16`). It works as a password.
- `target`: your Echo, e.g. `media_player.echo_show`. A list sends to several.
- `method`: `all` (voice + screen), `speak` (voice only) or `show` (screen only, on an Echo Show).
- The time window in `conditions`, if you like.

## 4. wt-monitor

```bash
install -m755 contrib/home-assistant/wt-notify-ha.sh ~/.local/bin/
( umask 077; echo 'http://127.0.0.1:8123/api/webhook/<your-webhook-id>' > ~/.config/wt-monitor/ha-webhook )
systemctl --user edit wt-monitor
```

In the editor:

```ini
[Service]
Environment=WT_NOTIFY_CMD=%h/.local/bin/wt-notify-ha.sh
Environment=WT_NOTIFY_EVENTS=ask
```

Then `systemctl --user restart wt-monitor` (this closes terminals started by the panel and resets priorities). Use `127.0.0.1` if Home Assistant runs on the same machine, otherwise its address on your network.

## 5. Test

```bash
WT_NOTIFY_JSON='{"event":"done","worktree":"test","title":"test","body":"test","urgent":false,"ts":0}' ~/.local/bin/wt-notify-ha.sh
```

Home Assistant answers the webhook with success even when the automation fails, so if nothing happens, check its log (*Settings → System → Logs*). `Action notify.alexa_media not found` means Alexa Media Player is not loaded (it sometimes times out after a restart): reload it in *Devices & services*.
