#!/usr/bin/env python3
"""Capture a landed instruction as a builder-replay fixture.

Replaying a real landed instruction is the only check that catches a builder
which reverts on current mainnet — a green unit suite does not. The fixture
freezes the account order and data of an instruction that actually executed.

    ./scripts/capture_instruction.py \
        --signature 2Z3gpRuN... \
        --program 6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P \
        --instruction buy \
        --out fixtures/pumpfun/ix_buy.json

Inner (CPI) instructions are the norm for swaps — routers own the top level —
so this searches inner instructions too and records which it found.

Stdlib only. RPC comes from --rpc or $SOLANA_RPC_URL.
"""
import argparse, base64, datetime, json, os, sys, urllib.request

B58 = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"


def b58decode(s):
    n = 0
    for c in s:
        n = n * 58 + B58.index(c)
    raw = n.to_bytes((n.bit_length() + 7) // 8, "big")
    return b"\0" * (len(s) - len(s.lstrip("1"))) + raw


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
    ap.add_argument("--signature", required=True)
    ap.add_argument("--program", required=True)
    ap.add_argument("--instruction", required=True, help="name, e.g. buy")
    ap.add_argument("--out", required=True)
    ap.add_argument("--index", type=int, help="pick the Nth match (default: 0)", default=0)
    ap.add_argument("--rpc", default=os.environ.get("SOLANA_RPC_URL"))
    a = ap.parse_args()
    if not a.rpc:
        sys.exit("need --rpc or $SOLANA_RPC_URL")

    tx = rpc(a.rpc, "getTransaction", [
        a.signature,
        {"encoding": "jsonParsed", "maxSupportedTransactionVersion": 0, "commitment": "finalized"},
    ])
    if not tx:
        sys.exit(f"transaction {a.signature} not found")
    if tx["meta"].get("err"):
        sys.exit(f"transaction failed on chain ({tx['meta']['err']}) — a fixture must be a LANDED success")

    # Collect candidates: top-level first, then inner. `top_level` is recorded
    # because jsonParsed signer/writable flags are MESSAGE-level and only
    # authoritative for a top-level instruction; the golden test asserts flags
    # only when this is true. It is always written explicitly — a missing key
    # would silently default to false and skip the assertion.
    cands = []
    for ix in tx["transaction"]["message"]["instructions"]:
        if ix.get("programId") == a.program and "accounts" in ix:
            cands.append((ix, True))
    for grp in tx["meta"].get("innerInstructions", []):
        for ix in grp["instructions"]:
            if ix.get("programId") == a.program and "accounts" in ix:
                cands.append((ix, False))

    if not cands:
        sys.exit(f"no instruction for program {a.program} in {a.signature}")
    if a.index >= len(cands):
        sys.exit(f"--index {a.index} out of range ({len(cands)} matches)")
    ix, top_level = cands[a.index]

    data = b58decode(ix["data"])
    keys = tx["transaction"]["message"]["accountKeys"]
    meta = {k["pubkey"]: k for k in keys}

    accounts = []
    for pk in ix["accounts"]:
        m = meta.get(pk, {})
        accounts.append({"pubkey": pk, "signer": bool(m.get("signer")), "writable": bool(m.get("writable"))})

    fixture = {
        "program": a.program,
        "instruction": a.instruction,
        "signature": a.signature,
        "slot": tx["slot"],
        "captured_at": datetime.date.today().isoformat(),
        "top_level": top_level,
        "discriminator": list(data[:8]),
        "data_b64": base64.b64encode(data).decode(),
        "accounts": accounts,
    }
    os.makedirs(os.path.dirname(a.out) or ".", exist_ok=True)
    with open(a.out, "w") as f:
        json.dump(fixture, f, indent=2)
        f.write("\n")
    print(f"wrote {a.out}  {len(accounts)} accounts  top_level={top_level}  ({len(cands)} match(es) in tx)")


if __name__ == "__main__":
    main()
