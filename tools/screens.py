#!/usr/bin/env python3
"""Screenshots of every screen of the game, for UI reviews (docs/UX.md).

Runs the development build of the game offscreen (Xvfb, Mesa's software
renderer) once per screen, language and window size, against a local server
full of made-up players (crates/server/examples/ux_seed.rs), and writes
`<out>/<size>/<lang>/<screen>.png`.

    cargo build -p snook-legend
    cargo build -p snook_legend_server --example ux_seed
    python3 tools/screens.py --out /tmp/shots                # everything
    python3 tools/screens.py --only menu,settings --langs en --sizes 1280x720

What moves (scrolling, a screen or a dialog coming in) is seen frame by
frame: --input plays the pointer from a script (interface units; frames
counted from the start) and --shots saves those frames too, as
`<screen>-<frame>.png`, at a fixed 60 frames a second whatever the renderer:

    python3 tools/screens.py --only ranking --langs en --sizes 1280x720 \
        --input "180:move 640 450;182:wheel -1" --shots 181,184,187,190,195

Needs Xvfb (`xvfb-run`). The binaries are looked for in $CARGO_TARGET_DIR
(or ./target)/debug. Release builds ignore the development switches used
here, so it must be a debug build.
"""

import argparse
import concurrent.futures
import os
import pathlib
import queue
import shutil
import subprocess
import sys
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parent.parent
PORT = 7979  # the first worker's server; the next ones count up

# name: (arguments, environment, online, frames)
# online: the game signs in as Ana on the local server. Her pinned message
# from the staff (a dialog until read) is kept off every screen but
# "staff-message" (SNOOK_LEGEND_QUIET_INBOX), where her new achievement's
# card waits behind it; the items her achievements brought, a dialog too,
# only on "staff-message" and "reward" (SNOOK_LEGEND_QUIET_REWARDS).
SCREENS = {
    "menu": ([], {}, False, 90),
    "menu-online": ([], {}, True, 150),
    "staff-message": ([], {"_loud": "1"}, True, 150),
    # The items an achievement brought ("Congratulations!").
    "reward": ([], {"_rewards": "1"}, True, 150),
    "whats-new": ([], {"SNOOK_LEGEND_WHATS_NEW": "1"}, False, 90),
    "rules": ([], {"SNOOK_LEGEND_WHATS_NEW": "rules"}, False, 90),
    "setup-free": (["--screen", "setup"], {}, False, 90),
    "setup-ladder": (["--screen", "ladder"], {}, False, 90),
    "setup-cup": (["--screen", "cup"], {}, False, 90),
    "setup-drills": (["--screen", "drills"], {}, True, 150),
    "setup-daily": (["--screen", "challenge"], {}, True, 150),
    "setup-clock": (["--screen", "clock"], {}, True, 150),
    "hotseat": (["--screen", "hotseat"], {}, False, 90),
    "lan": (["--screen", "lan"], {}, False, 90),
    "settings-video": (["--settings"], {}, False, 90),
    "settings-game": (["--settings"], {"SNOOK_LEGEND_SETTINGS_TAB": "game"}, False, 90),
    "settings-controls": (["--settings"], {"SNOOK_LEGEND_SETTINGS_TAB": "controls"}, False, 90),
    "settings-sound": (["--settings"], {"SNOOK_LEGEND_SETTINGS_TAB": "sound"}, False, 90),
    "settings-advanced": (["--settings"], {"SNOOK_LEGEND_SETTINGS_ADVANCED": "1"}, False, 90),
    "avatars": (["--screen", "avatars"], {}, False, 90),
    "signin": (["--online"], {}, False, 120),
    "lounge-players": (["--screen", "players"], {}, True, 200),
    "lounge-tables": (["--screen", "tables"], {}, True, 200),
    "lounge-tournaments": (["--screen", "tournaments"], {}, True, 200),
    "lounge-friends": (["--screen", "friends"], {}, True, 200),
    "lounge-daily": (["--screen", "daily"], {}, True, 200),
    "lounge-doubles": (["--screen", "doubles"], {}, True, 200),
    "inbox": (["--screen", "inbox"], {}, True, 200),
    "dm": (["--screen", "dm:Bruno"], {"SNOOK_LEGEND_OPEN": "dm:Bruno"}, True, 220),
    "ranking": (["--screen", "ranking"], {}, True, 200),
    "ranking-friends": (["--screen", "ranking-friends"], {}, True, 200),
    "ranking-doubles": (["--screen", "ranking-doubles"], {}, True, 200),
    "profile-games": (["--screen", "profile:Ana"], {}, True, 200),
    "profile-rating": (["--screen", "profile:Ana"], {"SNOOK_LEGEND_PROFILE_PANE": "rating"}, True, 200),
    "profile-numbers": (["--screen", "profile:Ana"], {"SNOOK_LEGEND_PROFILE_PANE": "numbers"}, True, 200),
    "profile-analysis": (["--screen", "profile:Ana"], {"SNOOK_LEGEND_PROFILE_PANE": "analysis"}, True, 200),
    "profile-achievements": (["--screen", "profile:Ana"], {"SNOOK_LEGEND_PROFILE_PANE": "achievements"}, True, 200),
    "profile-journey": (["--screen", "profile:Ana"], {"SNOOK_LEGEND_PROFILE_PANE": "journey"}, True, 200),
    "profile-other": (["--screen", "profile:Kenji"], {}, True, 200),
    "shop": (["--screen", "shop"], {}, True, 200),
    "shop-cues": (["--screen", "shop-cues"], {}, True, 200),
    "shop-coins": (["--screen", "shop-coins"], {}, True, 200),
    "shop-vip": (["--screen", "shop-vip"], {}, True, 200),
    "shop-confirm": (["--screen", "shop-confirm"], {}, True, 200),
    "auctions": (["--screen", "auctions"], {"SNOOK_LEGEND_OPEN": "auctions"}, True, 220),
    "credits": (["--screen", "credits"], {"SNOOK_LEGEND_OPEN": "credits"}, True, 220),
    "collection": (["--screen", "collection"], {}, True, 200),
    "match-8ball": (["--vs", "medium"], {}, False, 150),
    "match-official": (["--vs", "medium"], {"_ini": "rules=official"}, False, 150),
    "match-3d": (["--vs", "medium"], {"SNOOK_LEGEND_VIEW": "3d"}, False, 150),
    "match-call8": (["--vs", "medium", "--on-eight"], {"_ini": "rules=official"}, False, 150),
    "match-call8-br": (["--vs", "medium", "--on-eight"], {}, False, 150),
    "match-intro": (["--vs", "medium"], {"SNOOK_LEGEND_INTRO": "1"}, False, 90),
    # The introduction's animation midway: the cards sliding in, the VS
    # landing, the way out.
    "match-intro-in": (["--vs", "medium"], {"SNOOK_LEGEND_INTRO": "0.24s"}, False, 90),
    "match-intro-vs": (["--vs", "medium"], {"SNOOK_LEGEND_INTRO": "0.5s"}, False, 90),
    "match-intro-out": (["--vs", "medium"], {"SNOOK_LEGEND_INTRO": "3.2s"}, False, 140),
    "match-paused": (["--vs", "medium"], {"SNOOK_LEGEND_PAUSED": "1"}, False, 120),
    "match-result": (["--vs", "medium"], {"SNOOK_LEGEND_RESIGN": "1"}, False, 260),
    "match-hotseat": (["--hotseat"], {}, False, 150),
    # Online games against the seed's players who take any invitation
    # (one each, so a game still closing on the server doesn't hold the next).
    "match-online": ([], {"SNOOK_LEGEND_AUTOINVITE": "Wei"}, True, 420),
    "match-chat": ([], {"SNOOK_LEGEND_AUTOINVITE": "Aisha", "SNOOK_LEGEND_CHAT_OPEN": "1"}, True, 420),
    "match-reactions": (
        [],
        {"SNOOK_LEGEND_AUTOINVITE": "Tomasz", "SNOOK_LEGEND_OPEN_REACTIONS": "1"},
        True,
        420,
    ),
    "replay": ([], {"SNOOK_LEGEND_SAMPLE_REPLAY": "1"}, False, 150),
    "spectator": ([], {"SNOOK_LEGEND_SAMPLE_REPLAY": "1", "SNOOK_LEGEND_TV": "1"}, False, 150),
    "toasts": (["--screen", "toasts"], {}, True, 150),
}

LANGS = ["en", "pt-BR", "de", "ar"]
SIZES = ["1280x720", "1920x1080", "1024x600"]


def target_dir():
    return pathlib.Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")) / "debug"


def start_server(tmp, port):
    """A seeded server of its own per worker: Ana signed in on two screens
    at once would sign herself out of the first."""
    tokens = pathlib.Path(tmp) / f"tokens-{port}.txt"
    exe = target_dir() / "examples" / "ux_seed"
    proc = subprocess.Popen(
        [str(exe), str(port), str(tokens)],
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        cwd=tmp,
    )
    for _ in range(200):
        if tokens.exists() and tokens.stat().st_size > 0:
            break
        time.sleep(0.1)
    time.sleep(1.5)
    found = dict(
        line.split("=", 1) for line in tokens.read_text().splitlines() if "=" in line
    )
    return proc, found


def settings(lang, online, token, port):
    server = f"ws://127.0.0.1:{port}/ws"
    lines = [
        f"language={lang}",
        "coached=true",
        "graphics_checked=true",
        "auto_update=false",
        "last_version=99.0.0",
        "player_name=Ana",
        f"server={server}",
        "quality=high",
    ]
    if online:
        lines += [f"username=Ana", f"token={token}", f"token_server={server}"]
        # Newer than the account's settings on the server: the language
        # (and the rest) set here stay, instead of the account's.
        lines += ["settings_account=Ana", "settings_changed_at=4102444800"]
    return "\n".join(lines) + "\n"


def shoot(job, slot):
    name, lang, size, out = job
    port, token = slot
    args, env_extra, online, frames = SCREENS[name]
    frames = int(os.environ.get("SCREENS_FRAMES") or frames)
    w, h = size.split("x")
    dest = pathlib.Path(out) / size / lang / f"{name}.png"
    dest.parent.mkdir(parents=True, exist_ok=True)
    home = tempfile.mkdtemp(prefix="screens-")
    try:
        cfg = pathlib.Path(home) / "snook-legend"
        cfg.mkdir()
        ini = settings(lang, online, token, port) + env_extra.get("_ini", "") + "\n"
        (cfg / "settings.ini").write_text(ini)
        env = dict(os.environ)
        env.update(
            {
                "XDG_CONFIG_HOME": home,
                "SNOOK_LEGEND_MANY": "1",
                "SNOOK_LEGEND_SEED": "7",
                "LIBGL_ALWAYS_SOFTWARE": "1",
            }
        )
        if online and "_loud" not in env_extra:
            env["SNOOK_LEGEND_QUIET_INBOX"] = "1"
            if "_rewards" not in env_extra:
                env["SNOOK_LEGEND_QUIET_REWARDS"] = "1"
        env.update({k: v for k, v in env_extra.items() if not k.startswith("_")})
        cmd = [
            "xvfb-run",
            "-a",
            "-s",
            f"-screen 0 {w}x{h}x24",
            str(target_dir() / "snook-legend"),
            "--window",
            size,
            "--screenshot",
            str(dest),
            "--frames",
            str(frames),
        ] + args
        r = subprocess.run(cmd, env=env, capture_output=True, timeout=int(os.environ.get("SCREENS_TIMEOUT", "240")))
        ok = dest.exists()
        return name, lang, size, ok, (r.stderr.decode(errors="replace")[-400:] if not ok else "")
    except subprocess.TimeoutExpired:
        return name, lang, size, False, "timeout"
    finally:
        shutil.rmtree(home, ignore_errors=True)


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--out", default="screens")
    p.add_argument("--only", default="")
    p.add_argument("--langs", default=",".join(LANGS))
    p.add_argument("--sizes", default=",".join(SIZES))
    p.add_argument("--jobs", type=int, default=3)
    p.add_argument("--port", type=int, default=PORT, help="the first server's port (two runs at once need different ones)")
    p.add_argument("--input", default="", help="the pointer's script: '40:move 600 300;42:down;43:up;50:wheel -1'")
    p.add_argument("--shots", default="", help="more frames to save on the way: '181,184,190'")
    p.add_argument("--frames", type=int, default=0, help="the last frame (the screenshot), instead of the screen's own")
    p.add_argument("--timeout", type=int, default=240, help="seconds a shot may take (a busy machine needs more)")
    a = p.parse_args()
    os.environ["SCREENS_TIMEOUT"] = str(a.timeout)
    if a.frames:
        os.environ["SCREENS_FRAMES"] = str(a.frames)
    if a.input or a.shots:
        # Frame by frame: time runs at 60 frames a second, however slow.
        os.environ["SNOOK_LEGEND_DT"] = "0.016667"
        if a.input:
            os.environ["SNOOK_LEGEND_INPUT"] = a.input
        if a.shots:
            os.environ["SNOOK_LEGEND_SHOTS"] = a.shots
    names = [n for n in a.only.split(",") if n] or list(SCREENS)
    for n in names:
        if n not in SCREENS:
            sys.exit(f"unknown screen {n}; known: {', '.join(SCREENS)}")
    tmp = tempfile.mkdtemp(prefix="screens-server-")
    servers = []
    slots = queue.Queue()
    online = any(SCREENS[n][2] for n in names)
    for k in range(a.jobs):
        port = a.port + k
        token = ""
        if online:
            proc, tokens = start_server(tmp, port)
            servers.append(proc)
            token = tokens.get("Ana", "")
        slots.put((port, token))

    def run(job):
        slot = slots.get()
        try:
            return shoot(job, slot)
        finally:
            slots.put(slot)

    jobs = [
        (n, lang, size, a.out)
        for size in a.sizes.split(",")
        for lang in a.langs.split(",")
        for n in names
    ]
    failed = 0
    try:
        with concurrent.futures.ThreadPoolExecutor(a.jobs) as pool:
            for name, lang, size, ok, err in pool.map(run, jobs):
                print(f"{'ok ' if ok else 'ERR'} {size} {lang} {name} {err}", flush=True)
                failed += not ok
    finally:
        for proc in servers:
            proc.kill()
        shutil.rmtree(tmp, ignore_errors=True)
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
