#!/usr/bin/env python3
"""Install or remove the wt-monitor hooks (Stop, Notification, PreToolUse, PostToolUse) in Claude Code config dirs.

Usage: install-hooks.py [--uninstall] [config-dir ...]
Without directories it targets ~/.claude and every ~/.claude-*. Idempotent; every
changed settings.json is backed up first (settings.json.bak-wt-monitor on install,
settings.json.bak-wt-monitor-uninstall on uninstall).
"""
import glob, json, os, shutil, sys

home = os.path.expanduser("~")
cmd = os.path.join(home, ".local/bin/wt-agent-hook")
src = os.path.join(os.path.dirname(os.path.realpath(__file__)), "hook/wt-agent-hook")

argv = sys.argv[1:]
uninstall = "--uninstall" in argv
explicit = [os.path.expanduser(a) for a in argv if a != "--uninstall"]
dirs = explicit or [os.path.join(home, ".claude")] + sorted(glob.glob(os.path.join(home, ".claude-*")))


# Evento → matcher (None = sem matcher). O mesmo script trata todos pelo hook_event_name.
EVENTS = {
    "Stop": None,
    "Notification": "permission_prompt|elicitation_dialog",
    "PreToolUse": "AskUserQuestion",
    "PostToolUse": "AskUserQuestion",
}


def ours(h):
    return h.get("command") == cmd


def save(p, cfg):
    tmp = p + ".tmp"
    with open(tmp, "w") as f:
        json.dump(cfg, f, indent=2, ensure_ascii=False)
    os.replace(tmp, p)


if not uninstall:
    os.makedirs(os.path.dirname(cmd), exist_ok=True)
    shutil.copyfile(src, cmd)
    os.chmod(cmd, 0o755)

for d in dirs:
    p = os.path.join(d, "settings.json")
    if not os.path.isdir(d):
        continue
    cfg = json.load(open(p)) if os.path.exists(p) else {}

    hooks_cfg = cfg.setdefault("hooks", {})
    if uninstall:
        changed = False
        for ev in EVENTS:
            groups = hooks_cfg.get(ev, [])
            kept = []
            for g in groups:
                hs = [h for h in g.get("hooks", []) if not ours(h)]
                if hs:
                    kept.append({**g, "hooks": hs})
            if kept != groups:
                changed = True
                if kept:
                    hooks_cfg[ev] = kept
                else:
                    del hooks_cfg[ev]
        if not changed:
            print(f"não instalado: {p}")
            continue
        if not hooks_cfg:
            del cfg["hooks"]
        shutil.copyfile(p, p + ".bak-wt-monitor-uninstall")
        save(p, cfg)
        print(f"removido: {p}")
        continue

    missing = [
        ev for ev in EVENTS
        if not any(ours(h) for g in hooks_cfg.get(ev, []) for h in g.get("hooks", []))
    ]
    if not missing:
        print(f"já instalado: {p}")
        continue
    if os.path.exists(p):
        shutil.copyfile(p, p + ".bak-wt-monitor")
    for ev in missing:
        group = {"hooks": [{"type": "command", "command": cmd, "timeout": 5}]}
        if EVENTS[ev]:
            group = {"matcher": EVENTS[ev], **group}
        hooks_cfg.setdefault(ev, []).append(group)
    save(p, cfg)
    print(f"instalado ({', '.join(missing)}): {p}")

if uninstall and not explicit and os.path.exists(cmd):
    os.remove(cmd)
    print(f"removido: {cmd}")
