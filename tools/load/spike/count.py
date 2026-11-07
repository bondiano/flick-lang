#!/usr/bin/env python3
# Counts std's foreign bindings of tier B by what the bridge must convert. Input: dump.tsv, from
# FLICK_BRIDGE_DUMP=dump.tsv flick --project packages/std check. Run from the spike's root.
import csv, os, re, sys
here = os.path.dirname(os.path.abspath(__file__))
std = os.path.join(here, "../../../packages/std/src")
mods = {os.path.relpath(os.path.join(r, f), std)[:-3].replace("/", ".") for r, _, fs in os.walk(std) for f in fs if f.endswith(".fl")}
# tier A: the closure in docs/research/tier-a.md, its table of modules
tier_a = {"Action","Atom","Cause","Failure","Fiber","Instances","Layer","List.Core","Platform","Prelude","Queue","RawCause","Reactivity.Atom","Ref","Schedule","Scope","Test","Ui","Ui.Dom"}
named = re.compile(r"^(Stream|Channel|ChannelSchema|Sink|Http|HttpApi|Server|Rpc|Cli|Schema|Codec|Sql|Tx|Otlp|Prometheus|Metric|Tracer|Logger|Log|Check|Checks|DevTools|FileSystem|Path|ChildProcess|Stdio|Terminal|Net|Runner)(\.|$)")
rows = {}
for line in open(os.path.join(here, "dump.tsv")):
    p = line.rstrip("\n").split("\t")
    if len(p) < 6: continue
    rows[(p[0], p[1])] = p
def classify(p):
    mod, binding, mode, shapes, result, row = p
    s = shapes + " " + result
    return {
        "fun_or_eff_param": bool(re.search(r"Fun|Eff", shapes)),
        "fun_or_eff_result": bool(re.search(r"Fun|Eff", result)),
        "nested": bool(re.search(r"(List|Option|Tuple|Record|Result)\(", s)) or "Record" in s or "Tuple" in s,
        "exit_failure": bool(re.search(r"\bExit\b|\bFailure\b", s)),
        "layer": "Layer" in s,
        "effect": mode == "Effect",
        "effect_no_async": mode == "Effect" and row in ("closed", "open"),
        "effect_closed": mode == "Effect" and row == "closed",
        "effect_open": mode == "Effect" and row == "open",
    }
def report(name, keep):
    sel = [p for k, p in rows.items() if keep(k[0])]
    cs = [classify(p) for p in sel]
    n = len(sel)
    c = lambda key: sum(1 for x in cs if x[key])
    anyb = sum(1 for x in cs if x["fun_or_eff_param"] or x["fun_or_eff_result"] or x["exit_failure"] or x["layer"])
    print(f"{name}: modules {len({p[0] for p in sel})} bindings {n}; fun/Eff in a parameter {c('fun_or_eff_param')}; fun/Eff in the result {c('fun_or_eff_result')}; needs any wrapper {anyb}; nested {c('nested')}; Exit/Failure {c('exit_failure')}; Layer {c('layer')}; foreign effect {c('effect')}, of them row without Async {c('effect_no_async')} (closed {c('effect_closed')}, open {c('effect_open')})")
report("all std", lambda m: m in mods)
report("tier A table", lambda m: m in tier_a)
report("tier B = std minus the tier A table", lambda m: m in mods and m not in tier_a)
report("tier B, the modules named in the plan", lambda m: m in mods and m not in tier_a and named.match(m))
for k, p in rows.items():
    pass
if len(sys.argv) > 1:
    for k, p in sorted(rows.items()):
        if k[0] in mods and k[0] not in tier_a and classify(p)[sys.argv[1]]: print(k[0], "|", k[1][:70], "|", p[3], "|", p[4], "|", p[5])
