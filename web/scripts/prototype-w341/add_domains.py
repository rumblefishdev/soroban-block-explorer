# PROTOTYPE W341 — adds each issuer's home domain (accounts.home_domain, prod
# CH via chq, read-only) to the pool fixture.
import json, subprocess, sys

pools = json.load(open(sys.argv[1]))
issuers = sorted({l["i"] for p in pools for l in p["legs"] if l.get("i")})
dom = {}
CH = 2000
for k in range(0, len(issuers), CH):
    chunk = issuers[k:k + CH]
    arr = "[" + ",".join("'" + i + "'" for i in chunk) + "]"
    sql = ("SELECT account_id, argMax(home_domain, last_seen_ledger) FROM accounts "
           f"WHERE account_id IN {arr} GROUP BY account_id FORMAT TSV")
    out = subprocess.run(["chq", sql], capture_output=True, text=True).stdout
    for line in out.splitlines():
        parts = line.split("\t")
        if len(parts) == 2 and parts[1] not in ("", "\\N"):
            dom[parts[0]] = parts[1]
for p in pools:
    for l in p["legs"]:
        if l.get("i") in dom:
            l["d"] = dom[l["i"]]
json.dump(pools, open(sys.argv[2], "w"), separators=(",", ":"))
print(len(issuers), "issuers,", len(dom), "with a home domain")
