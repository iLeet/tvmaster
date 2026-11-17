#!/usr/bin/env python3
"""Checks that every setting the server has is offered by BOTH the web admin and the Mac app.

It reads the field names of the server's settings structs and looks for each one
in the web sources (as written) and in the Mac sources (as written, or camelCase).
A setting that one of them lacks is listed; the exit code is 1 if there is any.
Settings that are deliberately not edited by hand go in IGNORE with the reason.
"""
import re, sys, pathlib

ROOT = pathlib.Path(__file__).resolve().parent.parent
STRUCTS = {
    "crates/tvmaster-core/src/options.rs": ["AudioPrefs", "SubPrefs", "VideoOptions", "BugOptions", "ClockOptions", "ShowTitleOptions", "TickerOptions", "OffAir", "ChannelOptions", "ShowOptions"],
    "crates/tvmaster-core/src/ads.rs": ["BumperToggles", "AdConfig"],
    "crates/tvmaster-core/src/lineup.rs": ["Block", "Season", "Event", "Lineup", "SignOff"],
    "crates/tvmaster-core/src/model.rs": ["ThemeChoice", "GuideColors", "GuideBackground", "GuideAppearance", "GuideLayout", "GuideHeader", "GuideTicker", "GuidePip", "GuideConfig", "Channel"],
}
IGNORE = {
    # stored by the server or set by the app itself, not a setting
    "now", "next", "on_air", "online", "id", "epoch_ms", "seed", "cursor",
    # a list the apps build from other pages
    "timezone",
}

def fields(path, name):
    text = (ROOT / path).read_text()
    m = re.search(r"pub struct %s\s*\{(.*?)\n\}" % name, text, re.S)
    if not m:
        return []
    return re.findall(r"^\s*pub (\w+):", m.group(1), re.M)

def camel(s):
    p = s.split("_")
    return p[0] + "".join(w.title() for w in p[1:])

def corpus(globs):
    out = []
    for g in globs:
        for f in ROOT.glob(g):
            out.append(f.read_text())
    return "\n".join(out)

web = corpus(["web/src/**/*.ts", "web/src/**/*.tsx"])
mac = corpus(["apps/macos/Sources/**/*.swift"])

missing = {"web": [], "mac": []}
seen = set()
for path, structs in STRUCTS.items():
    for st in structs:
        for f in fields(path, st):
            if f in IGNORE or (st, f) in seen:
                continue
            seen.add((st, f))
            in_web = re.search(r"\b%s\b" % re.escape(f), web)
            in_mac = re.search(r"\b%s\b" % re.escape(f), mac) or re.search(r"\b%s\b" % re.escape(camel(f)), mac) or re.search(r'"%s"' % re.escape(f), mac)
            if not in_web:
                missing["web"].append(f"{st}.{f}")
            if not in_mac:
                missing["mac"].append(f"{st}.{f}")

bad = False
for who in ("web", "mac"):
    if missing[who]:
        bad = True
        print(f"Not in the {who} app:")
        for m in missing[who]:
            print("  ", m)
if not bad:
    print("Every setting is offered by the web admin and the Mac app.")
sys.exit(1 if bad else 0)
