#!/usr/bin/env python3
"""Fetch contract code for ALL prod wasm hashes (hashes.csv) in batches of 200."""
import base64, csv, hashlib, json, pathlib, struct, sys, time, urllib.request

HERE = pathlib.Path(__file__).parent
OUT = HERE / "wasms"
OUT.mkdir(exist_ok=True)
RPCS = [
    "https://mainnet.sorobanrpc.com",
    "https://soroban-rpc.mainnet.stellar.gateway.fm",
    "https://rpc.ankr.com/stellar_soroban",
]
BATCH = 200


def ledger_key(h: str) -> str:
    return base64.b64encode(struct.pack(">I", 7) + bytes.fromhex(h)).decode()


def parse_code(entry_xdr_b64: str) -> bytes:
    b = base64.b64decode(entry_xdr_b64)
    off = 0
    (disc,) = struct.unpack_from(">I", b, off); off += 4
    assert disc == 7, f"not CONTRACT_CODE: {disc}"
    (ext,) = struct.unpack_from(">I", b, off); off += 4
    if ext == 1:
        off += 4 + 4 + 10 * 4  # v1.ext + costInputs.ext + 10x uint32
    off += 32
    (n,) = struct.unpack_from(">I", b, off); off += 4
    code = b[off:off + n]
    assert code[:4] == b"\x00asm", "payload is not wasm — XDR layout drifted"
    return code


def call(keys):
    payload = json.dumps({"jsonrpc": "2.0", "id": 1,
                          "method": "getLedgerEntries",
                          "params": {"keys": keys}}).encode()
    last = None
    for rpc in RPCS:
        req = urllib.request.Request(
            rpc, method="POST",
            headers={"Content-Type": "application/json",
                     "User-Agent": "sorobanscan-spike/0.1"},
            data=payload)
        try:
            with urllib.request.urlopen(req, timeout=60) as r:
                return json.load(r)
        except Exception as exc:  # noqa: BLE001
            last = exc
    raise RuntimeError(f"all RPCs failed: {last}")


def main() -> None:
    with open(HERE / "hashes.csv") as f:
        hashes = [row[0] for row in csv.reader(f) if row]
    todo = [h for h in hashes if not (OUT / f"{h}.wasm").exists()]
    print(f"{len(hashes)} hashes, {len(todo)} to fetch")
    fetched = missing = 0
    miss_list = []
    for i in range(0, len(todo), BATCH):
        chunk = todo[i:i + BATCH]
        resp = call([ledger_key(h) for h in chunk])
        got = {}
        for e in resp["result"].get("entries") or []:
            code = parse_code(e["xdr"])
            got[hashlib.sha256(code).hexdigest()] = code
        for h in chunk:
            if h in got:
                (OUT / f"{h}.wasm").write_bytes(got[h])
                fetched += 1
            else:
                missing += 1
                miss_list.append(h)
        print(f"batch {i // BATCH + 1}/{(len(todo) + BATCH - 1) // BATCH}: "
              f"+{len(got)} (total ok {fetched}, miss {missing})")
        time.sleep(0.3)
    (HERE / "missing.txt").write_text("\n".join(miss_list))
    print(f"done: {fetched} fetched, {missing} missing (expired/archived) -> missing.txt")


if __name__ == "__main__":
    main()
