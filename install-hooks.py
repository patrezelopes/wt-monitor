#!/usr/bin/env python3
"""Install or remove the wt-monitor Stop hook in Claude Code config dirs.

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

    if uninstall:
        stop = cfg.get("hooks", {}).get("Stop", [])
        kept = []
        for g in stop:
            hooks = [h for h in g.get("hooks", []) if not ours(h)]
            if hooks:
                kept.append({**g, "hooks": hooks})
        if kept == stop:
            print(f"não instalado: {p}")
            continue
        shutil.copyfile(p, p + ".bak-wt-monitor-uninstall")
        if kept:
            cfg["hooks"]["Stop"] = kept
        else:
            del cfg["hooks"]["Stop"]
            if not cfg["hooks"]:
                del cfg["hooks"]
        save(p, cfg)
        print(f"removido: {p}")
        continue

    stop = cfg.setdefault("hooks", {}).setdefault("Stop", [])
    if any(ours(h) for g in stop for h in g.get("hooks", [])):
        print(f"já instalado: {p}")
        continue
    if os.path.exists(p):
        shutil.copyfile(p, p + ".bak-wt-monitor")
    stop.append({"hooks": [{"type": "command", "command": cmd, "timeout": 5}]})
    save(p, cfg)
    print(f"instalado: {p}")

if uninstall and not explicit and os.path.exists(cmd):
    os.remove(cmd)
    print(f"removido: {cmd}")
