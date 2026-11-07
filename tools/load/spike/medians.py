#!/usr/bin/env python3
# Medians of a load.sh TSV: usage medians.py <file>...
import statistics as s, sys
for f in sys.argv[1:]:
    rows = [l.rstrip("\n").split("\t") for l in open(f) if l.strip()]
    print(f)
    for mode in sorted({r[0] for r in rows}):
        rs = [r for r in rows if r[0] == mode]
        col = lambda i: [float(r[i]) for r in rs if r[i]]
        print(f" {mode}: rps {col(2)} median {s.median(col(2)):.0f}; p95 median {s.median(col(4)):.2f}; rss peak {col(7)} median {s.median(col(7)):.1f}; VmHWM {col(8)} median {s.median(col(8)):.1f}; CPU us/answer {col(11)} median {s.median(col(11)):.0f}; load {col(9)}")
    ms = sorted({r[0] for r in rows})
    if len(ms) == 2:
        a, b = [[float(r[2]) for r in rows if r[0] == m] for m in ms]
        ra, rb = [[float(r[8]) for r in rows if r[0] == m] for m in ms]
        print(f" {ms[1]} / {ms[0]}: rps x{s.median(b)/s.median(a):.3f}; VmHWM x{s.median(rb)/s.median(ra):.3f}")
