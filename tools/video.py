#!/usr/bin/env python3
"""A short promotional video of the game: title card, a match against the
computer (the introduction, the break, a run of pots), the main menu, and
an end card with where to get it (docs/UX.md, "A promotional video").

The game scenes are recorded by the development build itself, frame by
frame (SNOOK_LEGEND_RECORD, crates/client/src/record.rs: every frame moves
the game's clock on by exactly 1/60 s and goes to ffmpeg through a pipe,
the sounds beside it), offscreen under Xvfb and Mesa's software renderer.
The cards are tools/video/cards.html, drawn frame by frame by Playwright's
Chromium (tools/video/cards.js). ffmpeg cuts it all together with
crossfades:

    cargo build -p snook-legend
    python3 tools/video.py --out /tmp/promo                  # 1920x1080
    python3 tools/video.py --out /tmp/promo --format square  # 1080x1080
    python3 tools/video.py --out /tmp/promo --lang pt-BR --format square,vertical

--format takes wide (1920x1080), square (1080x1080) and vertical
(1080x1920, for stories and status), several with commas. --lang is the
language of the game and of the cards (en by default; the cards know en
and pt-BR); another language's files are named for it
(snooklegend-promo-pt-square.mp4).

The pieces stay in <out>/work and are made again only when missing (delete
one to redo it). Recording a scene takes a few minutes per second of
video. The cut points below are for the default seed (a game where the
player breaks and runs the rack): another seed needs new ones. Release
builds ignore the development switches, so it must be a debug build.
"""

import argparse
import os
import pathlib
import shutil
import subprocess
import sys
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parent.parent
FPS = 60

# name: (arguments, environment, online, seconds skipped, seconds recorded)
# online: signed in as Ana on a local server full of made-up players
# (crates/server/examples/ux_seed.rs, as for tools/screens.py).
SCENES = {
    # The player (played by the computer, SNOOK_LEGEND_AUTOPLAY) against
    # Viktor, official rules, the broadcast camera, in the hall.
    "match": (
        ["--vs", "viktor"],
        {"SNOOK_LEGEND_AUTOPLAY": "1", "SNOOK_LEGEND_TV": "1"},
        False,
        0.0,
        56.0,
    ),
    "menu": ([], {}, True, 3.0, 4.0),
    # (Past the cards of the achievements Ana is told about on signing in.)
    "ranking": (["--screen", "ranking"], {}, True, 24.0, 2.5),
}

# The cut: (piece, from, to) in seconds of that piece; each piece fades
# into the next over FADE seconds (taken from both). The match's moments
# (seed 18): the introduction until 3.4 s, the break struck at 9.4 s, then
# the player's pots struck at 33.0 s, 43.0 s and 53.2 s (Viktor has a word
# about that last one).
FADE = 0.45
CUT = [
    ("title", 0.0, 2.6),
    ("match", 0.0, 3.8),
    ("match", 8.4, 12.8),
    ("match", 31.7, 34.6),
    ("match", 41.8, 45.2),
    ("match", 52.0, 55.6),
    ("menu", 2.0, 4.0),
    ("ranking", 0.2, 2.2),
    ("end", 0.0, 3.8),
]


def target_dir():
    return pathlib.Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")) / "debug"


FORMATS = {"wide": (1920, 1080), "square": (1080, 1080), "vertical": (1080, 1920)}


def settings(server, token, lang, tall):
    lines = [
        f"language={lang}",
        "coached=true",
        "graphics_checked=true",
        "auto_update=false",
        "last_version=99.0.0",
        "quality=high",
        "rules=official",
        # The hall around the table, not the dark stage.
        "focused_scene=false",
        "tv_view=true",
        "shot_camera=true",
        "player_name=Lucas",
        "avatar=pic_crown",
        f"server={server}",
    ]
    if tall:
        # A phone's frame: the interface at its largest, for its width.
        lines.append("ui_size=1.5")
    if token:
        lines += ["username=Ana", f"token={token}", f"token_server={server}"]
        # Newer than the account's settings on the server: these stay.
        lines += ["settings_account=Ana", "settings_changed_at=4102444800"]
    return "\n".join(lines) + "\n"


def start_server(tmp, port):
    tokens = pathlib.Path(tmp) / "tokens.txt"
    exe = target_dir() / "examples" / "ux_seed"
    proc = subprocess.Popen(
        [str(exe), str(port), str(tokens)],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        cwd=tmp,
    )
    for _ in range(200):
        if tokens.exists() and tokens.stat().st_size > 0:
            break
        time.sleep(0.1)
    time.sleep(1.5)
    found = dict(l.split("=", 1) for l in tokens.read_text().splitlines() if "=" in l)
    return proc, found.get("Ana", "")


def record(name, size, dest, seed, game, port, lang):
    args, env_extra, online, skip, seconds = SCENES[name]
    w, h = size
    home = tempfile.mkdtemp(prefix="video-")
    server = None
    try:
        # Offline scenes point at a server that isn't there.
        url, token = "ws://127.0.0.1:9/ws", ""
        if online:
            server, token = start_server(home, port)
            url = f"ws://127.0.0.1:{port}/ws"
        cfg = pathlib.Path(home) / "snook-legend"
        cfg.mkdir()
        (cfg / "settings.ini").write_text(settings(url, token, lang, h > w))
        env = dict(os.environ)
        env.update(
            {
                "XDG_CONFIG_HOME": home,
                "SNOOK_LEGEND_MANY": "1",
                "SNOOK_LEGEND_SEED": str(seed),
                "LIBGL_ALWAYS_SOFTWARE": "1",
                "SNOOK_LEGEND_RECORD": str(dest),
                "SNOOK_LEGEND_RECORD_FROM": str(round(skip * FPS)),
                "SNOOK_LEGEND_RECORD_FRAMES": str(round(seconds * FPS)),
                # Ana's pinned message and rewards are dialogs: kept away.
                "SNOOK_LEGEND_QUIET_INBOX": "1",
                "SNOOK_LEGEND_QUIET_REWARDS": "1",
            }
        )
        env.update(env_extra)
        cmd = [
            "xvfb-run",
            "-a",
            "-s",
            f"-screen 0 {w}x{h}x24",
            str(game),
            "--window",
            f"{w}x{h}",
        ] + args
        subprocess.run(cmd, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    finally:
        if server:
            server.kill()
        shutil.rmtree(home, ignore_errors=True)
    if not dest.exists():
        sys.exit(f"{name}: nothing recorded")


def card(name, size, dest, seconds, lang):
    w, h = size
    js = ROOT / "tools" / "video" / "cards.js"
    cmd = ["node", str(js), name, str(w), str(h), str(seconds), str(dest), str(FPS), lang]
    subprocess.run(cmd, check=True)


def assemble(pieces, cut, dest, size):
    """The cut with crossfades, the pictures and the game's sounds."""
    w, h = size
    inputs, filters, audio = [], [], []
    for i, (name, start, end) in enumerate(cut):
        k = len(inputs) // 2
        inputs += ["-i", str(pieces[name])]
        filters.append(
            f"[{k}:v]trim={start}:{end},setpts=PTS-STARTPTS,fps={FPS},"
            f"scale={w}:{h},format=yuv444p[v{i}]"
        )
        wav = pieces[name].with_suffix(".wav")
        if wav.exists():
            k = len(inputs) // 2
            inputs += ["-i", str(wav)]
            filters.append(
                f"[{k}:a]atrim={start}:{end},asetpts=PTS-STARTPTS,"
                f"aformat=sample_rates=48000:channel_layouts=stereo[a{i}]"
            )
        else:
            filters.append(
                f"anullsrc=r=48000:cl=stereo,atrim=0:{end - start}[a{i}]"
            )
        audio.append(f"[a{i}]")
    # Video: each piece fades into the next.
    last, offset = "[v0]", 0.0
    for i in range(1, len(cut)):
        offset += (cut[i - 1][2] - cut[i - 1][1]) - FADE
        out = f"[x{i}]"
        filters.append(
            f"{last}[v{i}]xfade=transition=fade:duration={FADE}:offset={offset:.3f}{out}"
        )
        last = out
    # Sound: the same crossfades.
    alast = audio[0]
    for i in range(1, len(cut)):
        out = f"[y{i}]"
        filters.append(f"{alast}{audio[i]}acrossfade=d={FADE}:c1=tri:c2=tri{out}")
        alast = out
    filters.append(f"{last}format=yuv420p[vout]")
    cmd = (
        ["ffmpeg", "-y", "-loglevel", "error"]
        + inputs
        + ["-filter_complex", ";".join(filters), "-map", "[vout]", "-map", alast]
        + ["-c:v", "libx264", "-preset", "slow", "-crf", "18", "-profile:v", "high"]
        + ["-r", str(FPS), "-pix_fmt", "yuv420p", "-movflags", "+faststart"]
        + ["-c:a", "aac", "-b:a", "160k", str(dest)]
    )
    subprocess.run(cmd, check=True)


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--out", default="promo")
    p.add_argument("--seed", type=int, default=18)
    p.add_argument("--format", default="wide", help="wide, square, vertical (several: a,b); both: wide,square")
    p.add_argument("--lang", default="en", help="the game's and the cards' language (en, pt-BR)")
    p.add_argument("--game", default=str(target_dir() / "snook-legend"))
    p.add_argument("--port", type=int, default=7989, help="the local server's (online scenes)")
    p.add_argument("--only", default="", help="record these scenes only (no cut)")
    a = p.parse_args()
    out = pathlib.Path(a.out).resolve()
    work = out / "work"
    work.mkdir(parents=True, exist_ok=True)
    kinds = "wide,square" if a.format == "both" else a.format
    # English keeps the plain names; another language's carry its code.
    lang = "" if a.lang == "en" else "-" + a.lang.split("-")[0].lower()
    formats = []
    for kind in kinds.split(","):
        if kind not in FORMATS:
            sys.exit(f"--format: {kind}? ({', '.join(FORMATS)})")
        suffix = "" if kind == "wide" else f"-{kind}"
        formats.append((f"snooklegend-promo{lang}{suffix}", FORMATS[kind]))
    for title, size in formats:
        tag = f"{size[0]}x{size[1]}{lang}"
        pieces = {}
        names = [n for n in a.only.split(",") if n] or sorted({c[0] for c in CUT})
        for name in names:
            dest = work / f"{name}-{tag}.mp4"
            pieces[name] = dest
            if dest.exists():
                continue
            print(f"{tag}: {name}", flush=True)
            if name in SCENES:
                record(name, size, dest, a.seed, a.game, a.port, a.lang)
            else:
                seconds = max(c[2] for c in CUT if c[0] == name)
                card(name, size, dest, seconds, a.lang)
        if a.only:
            continue
        assemble(pieces, CUT, out / f"{title}.mp4", size)
        print(out / f"{title}.mp4", flush=True)


if __name__ == "__main__":
    main()
