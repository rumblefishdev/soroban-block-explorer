# PROTOTYPE W341 — issuer home domains read from the chain (stellar CLI over a
# public mainnet RPC, read-only), for issuers our accounts table has none for
# (task 0469). Writes issuer<TAB>domain.
import json, subprocess, sys
from concurrent.futures import ThreadPoolExecutor

RPC = "https://soroban-rpc.mainnet.stellar.gateway.fm"
PASS = "Public Global Stellar Network ; September 2015"

def one(acc):
    r = subprocess.run(["stellar", "ledger", "entry", "fetch", "account", "--account", acc,
                        "--rpc-url", RPC, "--network-passphrase", PASS],
                       capture_output=True, text=True)
    try:
        e = json.loads(r.stdout)["entries"][0]["val"]["account"]
        return acc, e.get("home_domain") or ""
    except Exception:
        return acc, None

accs = open(sys.argv[1]).read().split()
with ThreadPoolExecutor(8) as ex:
    res = list(ex.map(one, accs))
with open(sys.argv[2], "w") as f:
    for a, d in res:
        if d is not None:
            f.write(f"{a}\t{d}\n")
print(len(accs), "asked,", sum(1 for _, d in res if d is not None), "answered,",
      sum(1 for _, d in res if d), "with a domain")
