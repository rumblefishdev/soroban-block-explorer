# PROTOTYPE W341 — pages the local API's pool list (prod CH) into a compact
# fixture: id, kind, fee, tvl, volume, legs [label, code, issuer|contract, type].
import json, sys, time, urllib.request, urllib.parse

BASE = "http://localhost:9100/v1/liquidity-pools"
out, cursor, page = [], None, 0
t0 = time.time()
while True:
    q = {"limit": "100"}
    if cursor:
        q["cursor"] = cursor
    with urllib.request.urlopen(BASE + "?" + urllib.parse.urlencode(q), timeout=120) as r:
        body = json.load(r)
    for p in body["data"]:
        legs = []
        for l in p["legs"]:
            legs.append({
                "t": l.get("asset_type_name"),
                "c": l.get("asset_code"),
                "i": l.get("issuer"),
                "k": l.get("contract_id"),
                "s": l.get("symbol"),
            })
        out.append({
            "id": p["pool_id"],
            "kind": p.get("pool_kind"),
            "fee": p.get("fee_percent"),
            "tvl": p.get("tvl"),
            "vol": p.get("volume"),
            "n": p.get("participant_count"),
            "proto": p.get("protocol"),
            "legs": legs,
        })
    page += 1
    cursor = body["page"].get("next_cursor")
    if page % 20 == 0:
        print(page, len(out), round(time.time() - t0), flush=True)
    if not cursor:
        break
json.dump(out, open(sys.argv[1], "w"), separators=(",", ":"))
print("done", len(out), round(time.time() - t0))
