#!/usr/bin/env python3
"""Capture a golden account fixture from mainnet.

A fixture freezes a real account's bytes so the decoder is verified against the
chain rather than against an IDL. See docs in `src/test_fixtures.rs`.

    ./scripts/capture_account.py \
        --address 4wTV1YmiEkRvAtNtsSGPtUrqRYQMe5SKy2uB4Jjaxnjf \
        --program 6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P \
        --type Global \
        --out fixtures/pumpfun/global.json \
        --note "pumpfun Global PDA"

Stdlib only. RPC comes from --rpc or $SOLANA_RPC_URL.
"""
import argparse, datetime, json, os, sys, urllib.request


def rpc(url, method, params):
    req = urllib.request.Request(
        url,
        data=json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).encode(),
        headers={"Content-Type": "application/json"},
    )
    with urllib.request.urlopen(req, timeout=30) as r:
        body = json.load(r)
    if "error" in body:
        sys.exit(f"RPC {method} failed: {body['error']}")
    return body["result"]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--address", required=True)
    ap.add_argument("--program", required=True, help="owner program id (asserted against chain)")
    ap.add_argument("--type", dest="account_type", required=True, help="e.g. Global, Pool")
    ap.add_argument("--out", required=True)
    ap.add_argument("--note", default="")
    ap.add_argument("--rpc", default=os.environ.get("SOLANA_RPC_URL"))
    a = ap.parse_args()
    if not a.rpc:
        sys.exit("need --rpc or $SOLANA_RPC_URL")

    res = rpc(a.rpc, "getAccountInfo", [a.address, {"encoding": "base64", "commitment": "finalized"}])
    if not res or not res.get("value"):
        sys.exit(f"account {a.address} not found")
    val, slot = res["value"], res["context"]["slot"]

    # The owner is half of an account's identity — a fixture captured from the
    # wrong program would pin a layout that never occurs under this decoder.
    if val["owner"] != a.program:
        sys.exit(f"owner mismatch: chain says {val['owner']}, you said {a.program}")

    data_b64 = val["data"][0]
    fixture = {
        "program": a.program,
        "account_type": a.account_type,
        "address": a.address,
        "slot": slot,
        "captured_at": datetime.date.today().isoformat(),
        "size": len(data_b64.encode()) and __import__("base64").b64decode(data_b64).__len__(),
        "note": a.note,
        "data_b64": data_b64,
        # Fill this in by hand with the fields this fixture must pin. An empty
        # `expected` is a fixture that asserts nothing — see the "machinery that
        # never runs" class in the project's learnings.
        "expected": {},
    }
    os.makedirs(os.path.dirname(a.out) or ".", exist_ok=True)
    with open(a.out, "w") as f:
        json.dump(fixture, f, indent=2)
        f.write("\n")
    print(f"wrote {a.out}  size={fixture['size']}  slot={slot}")
    print("NEXT: fill in `expected` with the fields to pin, then add the golden test.")


if __name__ == "__main__":
    main()
