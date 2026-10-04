#!/usr/bin/env python3
"""Compare original (marked) and Rust (pulldown-cmark) parser output."""
import json
import sys

corpus = json.load(open(sys.argv[1]))
orig = json.load(open(sys.argv[2]))
rust = json.load(open(sys.argv[3]))


def sec(s):
    return (s["type"], s["level"], s["content"], s["raw"].rstrip("\n"), s["imageRef"], s["refs"])


def spans(lines):
    return [[(x["t"], x["b"], x["i"], x["c"], x["s"], x["h"], x["f"]) for x in line] for line in lines]


same = 0
for i, (a, b) in enumerate(zip(orig, rust)):
    sa, sb = [sec(s) for s in a["sections"]], [sec(s) for s in b["sections"]]
    if sa == sb and a["footnotes"] == b["footnotes"] and spans(a["inline"]) == spans(b["inline"]):
        same += 1
        continue
    print(f"=== #{i}: {corpus[i]!r}")
    if sa != sb:
        print("  original:", sa)
        print("  rust:    ", sb)
    if a["footnotes"] != b["footnotes"]:
        print("  footnotes:", a["footnotes"], "|", b["footnotes"])
    if spans(a["inline"]) != spans(b["inline"]):
        print("  inline original:", spans(a["inline"]))
        print("  inline rust:    ", spans(b["inline"]))
print(f"\nidentical: {same}/{len(corpus)}")
