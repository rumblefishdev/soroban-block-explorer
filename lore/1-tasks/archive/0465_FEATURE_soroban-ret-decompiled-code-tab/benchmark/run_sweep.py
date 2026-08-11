#!/usr/bin/env python3
"""Full sweep: soroban-ret 0.0.4 over every fetched wasm; aggregate stats + results.csv."""
import csv, pathlib, re, statistics, subprocess, time

HERE = pathlib.Path(__file__).parent
WASMS = sorted((HERE / "wasms").glob("*.wasm"))
OUT = HERE / "rust"
OUT.mkdir(exist_ok=True)
BIN = pathlib.Path.home() / ".cargo" / "bin" / "soroban-ret"

rows = []
fails = []
for i, w in enumerate(WASMS, 1):
    rs = OUT / f"{w.stem}.rs"
    t0 = time.perf_counter()
    try:
        p = subprocess.run([str(BIN), str(w), "-o", str(rs)],
                           capture_output=True, text=True, timeout=120)
        ms = (time.perf_counter() - t0) * 1000
        if p.returncode != 0:
            err = (p.stderr or "").strip().splitlines()
            fails.append((w.stem, "error", err[-1][:100] if err else "?"))
            rows.append({"hash": w.stem, "wasm_b": w.stat().st_size, "ms": round(ms),
                         "status": "error", "fns": 0, "todos": 0, "varn": 0, "rust_b": 0})
            continue
    except subprocess.TimeoutExpired:
        fails.append((w.stem, "timeout", ">120s"))
        rows.append({"hash": w.stem, "wasm_b": w.stat().st_size, "ms": 120000,
                     "status": "timeout", "fns": 0, "todos": 0, "varn": 0, "rust_b": 0})
        continue
    src = rs.read_text()
    rows.append({"hash": w.stem, "wasm_b": w.stat().st_size, "ms": round(ms),
                 "status": "ok",
                 "fns": len(re.findall(r"\bpub fn \w+", src)),
                 "todos": len(re.findall(r"todo\s*!\s*\(", src)),
                 "varn": len(set(re.findall(r"\bvar_\d+\b", src))),
                 "rust_b": len(src)})
    if i % 250 == 0:
        print(f"...{i}/{len(WASMS)}")

with open(HERE / "results.csv", "w", newline="") as f:
    wtr = csv.DictWriter(f, fieldnames=rows[0].keys())
    wtr.writeheader()
    wtr.writerows(rows)

ok = [r for r in rows if r["status"] == "ok"]
times = sorted(r["ms"] for r in ok)
todos = [r["todos"] for r in ok]
holefree = [r for r in ok if r["todos"] == 0 and r["varn"] == 0]


def pct(xs, p):
    return xs[min(len(xs) - 1, int(len(xs) * p))]


print(f"\nwasms: {len(rows)}  ok: {len(ok)}  failed: {len(fails)}")
print(f"time ms: median {pct(times, .5)}  p90 {pct(times, .9)}  p99 {pct(times, .99)}  max {times[-1]}")
print(f"hole-free: {len(holefree)}/{len(ok)} ({100 * len(holefree) / len(ok):.0f}%)")
print(f"todos per contract: median {int(statistics.median(todos))}  mean {statistics.mean(todos):.0f}  max {max(todos)}")
if fails:
    print("\nfailures:")
    for h, kind, msg in fails[:20]:
        print(f"  {h[:8]} {kind}: {msg}")
    if len(fails) > 20:
        print(f"  ... +{len(fails) - 20} more")
