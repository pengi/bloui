#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""Check that every dependency's license is compatible with GPL-2.0-only.

Reads `cargo metadata` (all targets and platforms, so it is conservative)
and evaluates each package's SPDX license expression against an allow list.
Exits with status 1 and lists the offenders if any package doesn't pass.
"""

import json
import re
import subprocess
import sys

# Licenses that can be combined into a GPL-2.0-only program.
ALLOWED = {
    "MIT", "MIT-0", "ISC", "Zlib", "0BSD", "BSD-2-Clause", "BSD-3-Clause",
    "BSL-1.0", "Unlicense", "CC0-1.0", "Unicode-3.0", "Unicode-DFS-2016",
    # MPL-2.0 lists GPL-2.0 as a secondary license.
    "MPL-2.0",
    "GPL-2.0", "GPL-2.0-only", "GPL-2.0-or-later",
    "LGPL-2.0-or-later", "LGPL-2.1", "LGPL-2.1-only", "LGPL-2.1-or-later",
    # The LLVM exception was written for GPL-2.0 compatibility.
    "Apache-2.0 WITH LLVM-exception",
}
# Note: plain Apache-2.0 is NOT compatible with GPL-2.0-only.


def tokens(expr):
    expr = expr.replace("/", " OR ")
    return re.findall(r"\(|\)|[A-Za-z0-9.+\-]+", expr)


def parse(toks):
    """Evaluate an SPDX expression: OR binds weaker than AND."""

    def primary():
        t = toks.pop(0)
        if t == "(":
            v = disjunction()
            toks.pop(0)  # ")"
            return v
        name = t
        if toks and toks[0] == "WITH":
            toks.pop(0)
            name = f"{t} WITH {toks.pop(0)}"
        return name in ALLOWED or name.rstrip("+") in ALLOWED

    def conjunction():
        v = primary()
        while toks and toks[0] == "AND":
            toks.pop(0)
            v = primary() and v
        return v

    def disjunction():
        v = conjunction()
        while toks and toks[0] == "OR":
            toks.pop(0)
            v = conjunction() or v
        return v

    return disjunction()


def main():
    meta = json.loads(
        subprocess.check_output(["cargo", "metadata", "--format-version", "1", "--locked"])
    )
    root = {p["id"] for p in meta["packages"] if p["source"] is None}
    bad = []
    for p in meta["packages"]:
        if p["id"] in root:
            continue
        lic = p.get("license")
        if not lic or not parse(tokens(lic)):
            bad.append(f"{p['name']} {p['version']}: {lic or 'no license field'}")
    if bad:
        print("Dependencies not compatible with GPL-2.0-only:")
        print("\n".join(f"  {b}" for b in sorted(bad)))
        return 1
    print(f"ok: {len(meta['packages']) - len(root)} dependencies are GPL-2.0-only compatible")
    return 0


if __name__ == "__main__":
    sys.exit(main())
