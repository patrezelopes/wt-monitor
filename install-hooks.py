#!/usr/bin/env python3
"""Instala o hook Stop em cada pasta de config do Claude Code (idempotente, com backup)."""
import glob, json, os, shutil, sys

home = os.path.expanduser("~")
cmd = os.path.join(home, ".local/bin/wt-agent-hook")
src = os.path.join(os.path.dirname(os.path.realpath(__file__)), "hook/wt-agent-hook")
os.makedirs(os.path.dirname(cmd), exist_ok=True)
shutil.copyfile(src, cmd)
os.chmod(cmd, 0o755)

dirs = sys.argv[1:] or [os.path.join(home, ".claude")] + sorted(glob.glob(os.path.join(home, ".claude-*")))
for d in dirs:
    p = os.path.join(d, "settings.json")
    if not os.path.isdir(d):
        continue
    cfg = json.load(open(p)) if os.path.exists(p) else {}
    stop = cfg.setdefault("hooks", {}).setdefault("Stop", [])
    if any(h.get("command") == cmd for g in stop for h in g.get("hooks", [])):
        print(f"já instalado: {p}")
        continue
    if os.path.exists(p):
        shutil.copyfile(p, p + ".bak-wt-monitor")
    stop.append({"hooks": [{"type": "command", "command": cmd, "timeout": 5}]})
    tmp = p + ".tmp"
    json.dump(cfg, open(tmp, "w"), indent=2, ensure_ascii=False)
    os.replace(tmp, p)
    print(f"instalado: {p}")
