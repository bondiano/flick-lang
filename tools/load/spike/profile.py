#!/usr/bin/env python3
# Self time of a node --cpu-prof profile by where the code is: usage profile.py <mode>... for /tmp/prof/<mode>/*.cpuprofile
import json, sys, glob, collections
def cat(url, fn):
    if not url: return "(native/gc/idle)" if fn in ("(garbage collector)","(program)","(idle)") else "(native)"
    if "/runtime/dist/rt/bridge" in url: return "bridge"
    if "/runtime/dist/rt/" in url: return "kernel rt/*"
    if "/runtime/dist/" in url: return "runtime (Effect side)"
    if "/node_modules/" in url and "/effect/" in url: return "effect"
    if "/node_modules/" in url and ("react" in url or "scheduler" in url): return "react"
    if "/node_modules/" in url: return "other deps"
    if "/_flick/" in url: return "compiled std"
    if "/dist/" in url: return "program"
    if url.startswith("node:"): return "node"
    return "other"
for m in sys.argv[1:]:
    p = json.load(open(glob.glob(f"/tmp/prof/{m}/*.cpuprofile")[0]))
    nodes = {n["id"]: n for n in p["nodes"]}
    dt = collections.Counter()
    for s, d in zip(p["samples"], p["timeDeltas"]): dt[s] += d
    by = collections.Counter(); fns = collections.Counter()
    for nid, t in dt.items():
        cf = nodes[nid]["callFrame"]; c = cat(cf["url"], cf["functionName"])
        if cf["functionName"] == "(idle)": c = "(idle)"
        by[c] += t; fns[(c, cf["functionName"] or "(anon)", cf["url"].split("/")[-1], cf["lineNumber"])] += t
    busy = sum(t for c, t in by.items() if c != "(idle)")
    print(f"== {m}: busy {busy/1e6:.2f} s")
    for c, t in by.most_common():
        if c != "(idle)": print(f"  {c:24s} {t/1e6:6.2f} s {100*t/busy:5.1f}%")
    print("  top functions:")
    for k, t in fns.most_common(14):
        if k[0] != "(idle)": print(f"    {t/1e6:5.2f} s  {k[0]:20s} {k[1]} {k[2]}:{k[3]}")
