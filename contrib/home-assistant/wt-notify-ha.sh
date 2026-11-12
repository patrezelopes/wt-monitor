#!/bin/sh
# Envia uma notificação do wt-monitor para um webhook do Home Assistant.
# Uso: WT_NOTIFY_CMD=/caminho/wt-notify-ha.sh  (o wt-monitor define WT_NOTIFY_JSON e chama este script)
#
# A URL do webhook vem de WT_HA_WEBHOOK ou do arquivo ~/.config/wt-monitor/ha-webhook
# (uma linha, ex.: http://homeassistant.local:8123/api/webhook/wt-monitor-<algo-aleatório>).
# O id do webhook funciona como senha: guarde o arquivo com chmod 600.
set -eu

url="${WT_HA_WEBHOOK:-}"
if [ -z "$url" ]; then
    f="${XDG_CONFIG_HOME:-$HOME/.config}/wt-monitor/ha-webhook"
    [ -r "$f" ] && url=$(head -n1 "$f" | tr -d '[:space:]')
fi
if [ -z "$url" ]; then
    echo "wt-notify-ha: defina WT_HA_WEBHOOK ou crie ~/.config/wt-monitor/ha-webhook" >&2
    exit 1
fi
: "${WT_NOTIFY_JSON:?wt-notify-ha: WT_NOTIFY_JSON vazio (rode pelo wt-monitor)}"

exec curl -fsS --max-time 5 -o /dev/null \
    -H 'Content-Type: application/json' \
    --data-binary "$WT_NOTIFY_JSON" \
    "$url"
