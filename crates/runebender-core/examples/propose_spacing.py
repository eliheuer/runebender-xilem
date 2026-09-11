#!/usr/bin/env python3
"""Read a glyph and optionally propose a wider/narrower right sidebearing.

A thin client of the core tools, with no font library or model dependency.
"""
import argparse
import json
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("font")
    parser.add_argument("--master", type=int, required=True)
    parser.add_argument("--glyph", default="n")
    parser.add_argument("--delta", type=float, required=True)
    parser.add_argument("--task", required=True)
    parser.add_argument("--binary", default="runebender-core")
    parser.add_argument("--write", action="store_true")
    args = parser.parse_args()

    def call(name, parameters):
        process = subprocess.run(
            [args.binary, "agent", "call", name, "--font", args.font, "--args-file", "-"],
            input=json.dumps(parameters, allow_nan=False), text=True, capture_output=True,
            check=False,
        )
        if not process.stdout.strip():
            raise RuntimeError(process.stderr)
        result = json.loads(process.stdout)
        if process.returncode or not result["ok"]:
            raise RuntimeError(result)
        return result["result"]

    glyph = call("read_glyph", {"master": args.master, "glyph": args.glyph})
    batch = {
        "master": args.master, "task": args.task,
        "reason": f"Compare a {args.delta:+g}-unit change to the right sidebearing",
        "edits": [{"glyph": args.glyph, "expected_revision": glyph["revision"],
                   "operations": [{"op": "set_width", "width": glyph["advance"] + args.delta}]}],
    }
    if not args.write:
        print(json.dumps(batch, indent=2, allow_nan=False))
        return
    result = call("propose_edits", batch)
    proofs = {}
    for label, extra in (("before", {}), ("proposal", {"layer": result["proposal"]["layer"]})):
        proof = call("proof", {"master": args.master, "glyphs": [args.glyph], **extra})
        proofs[label] = {"svg": proof["svg"], "metrics": proof["glyphs"]}
    print(json.dumps({"proposal": result, "proofs": proofs}, indent=2))


if __name__ == "__main__":
    main()
