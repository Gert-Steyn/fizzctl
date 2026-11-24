"""Decrypt and de-obfuscate the IllumiPC web configurator's app.js.

The configurator's boot.js downloads ./app.js?v=<n>, which is
`WMJ1` + XOR("witmod-app-0x5a"). The decrypted JS is javascript-obfuscator
output: strings live in a base64-encoded array behind a lookup function,
and the array is rotated at startup. This script undoes all of that
statically. It never executes the vendor's code.

Usage (Python 3, run isolated because the input is untrusted):

    curl -sS -A "Mozilla/5.0" -o app.js.enc "https://www.illumipc.com/app.js"
    python -I tools/decode_app.py app.js.enc app.deob.js strings.txt

Then search app.deob.js, for example for `new Uint8Array(0x3f)` (every
command packet) or `class adk` (input packet dispatcher). Identifier names
change on every vendor rebuild; the patterns below detect them.
"""

import json
import re
import sys

APP_MAGIC = b"WMJ1"
APP_KEY = b"witmod-app-0x5a"
B64 = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789+/="


def decrypt(data: bytes) -> str:
    if data[:4] == APP_MAGIC:
        body = data[4:]
        data = bytes(b ^ APP_KEY[i % len(APP_KEY)] for i, b in enumerate(body))
    return data.decode("utf-8")


def parse_js_strings(literal: str) -> list[str]:
    out, p = [], 0
    while p < len(literal):
        c = literal[p]
        if c not in "'\"":
            p += 1
            continue
        quote, p, buf = c, p + 1, []
        while literal[p] != quote:
            if literal[p] == "\\":
                n = literal[p + 1]
                if n == "x":
                    buf.append(chr(int(literal[p + 2 : p + 4], 16)))
                    p += 4
                elif n == "u":
                    buf.append(chr(int(literal[p + 2 : p + 6], 16)))
                    p += 6
                else:
                    buf.append({"n": "\n", "t": "\t", "r": "\r", "0": "\0"}.get(n, n))
                    p += 2
                continue
            buf.append(literal[p])
            p += 1
        out.append("".join(buf))
        p += 1
    return out


def b64_decode(s: str) -> str:
    """javascript-obfuscator's base64 variant (lower-case alphabet first)."""
    out, bits, count = bytearray(), 0, 0
    for ch in s:
        q = B64.find(ch)
        if q < 0 or q == 64:
            continue
        bits = bits * 64 + q if count % 4 else q
        count += 1
        if (count - 1) % 4:
            out.append(0xFF & (bits >> ((-2 * count) & 6)))
    try:
        return out.decode("utf-8")
    except UnicodeDecodeError:
        return out.decode("latin1")


def main() -> None:
    src, dst, strings_out = sys.argv[1:4]
    s = decrypt(open(src, "rb").read())

    arr_fn = re.search(r"function (\w+)\(\)\{const ([\w$]+)=\[", s)
    arr_name, arr_var = arr_fn.group(1), arr_fn.group(2)
    start = arr_fn.end()
    end = s.index("];" + arr_name + "=function(){return " + arr_var + ";}", start)
    strings = parse_js_strings(s[start:end])

    lookup = re.search(r"function ([\w$]+)\(c,d\)\{c=c-(0x[0-9a-f]+);", s)
    lookup_name, offset = lookup.group(1), int(lookup.group(2), 16)

    rot_iife = re.search(
        r"\(function\(c,d\)\{const ([\w$]+)=" + re.escape(lookup_name) + r",e=c\(\);", s
    )
    rot_alias = rot_iife.group(1)
    tail = s[rot_iife.start() : rot_iife.start() + 3000]
    expr = re.search(r"const f=(.*?);if\(f===d\)break", tail).group(1)
    target = int(re.search(r"\}\(" + arr_name + r",(0x[0-9a-f]+)\)\)", tail).group(1), 16)

    def eval_rotation(rot: int) -> float:
        rotated = strings[rot:] + strings[:rot]

        def num(m):
            text = b64_decode(rotated[int(m.group(1), 16) - offset])
            lead = re.match(r"\s*([+-]?\d+)", text)
            if not lead:
                raise ValueError
            return lead.group(1)

        e = re.sub(r"parseInt\(" + re.escape(rot_alias) + r"\((0x[0-9a-f]+)\)\)", num, expr)
        e = re.sub(r"0x[0-9a-f]+", lambda m: str(int(m.group(0), 16)), e)
        return eval(e, {"__builtins__": {}})  # arithmetic only, built from numbers

    rotation = next(
        r for r in range(len(strings)) if _safe(lambda: abs(eval_rotation(r) - target) < 1e-6)
    )
    decoded = [b64_decode(x) for x in strings[rotation:] + strings[:rotation]]
    print(f"strings {len(decoded)}, rotation {rotation}, offset {offset:#x}", file=sys.stderr)

    # Every `x=lookup` alias, transitively, in one pass.
    pairs: dict[str, set[str]] = {}
    for m in re.finditer(r"([\w$]+)=([\w$]+)(?=[,;])", s):
        pairs.setdefault(m.group(2), set()).add(m.group(1))
    aliases, stack = {lookup_name}, [lookup_name]
    while stack:
        for alias in pairs.get(stack.pop(), ()):
            if alias not in aliases:
                aliases.add(alias)
                stack.append(alias)

    def replace(m):
        if m.group(1) not in aliases:
            return m.group(0)
        idx = int(m.group(2), 16) - offset
        return json.dumps(decoded[idx], ensure_ascii=False) if 0 <= idx < len(decoded) else m.group(0)

    out = re.sub(r"(?<![\w$.])([\w$]+)\((0x[0-9a-f]+)\)", replace, s[end:])
    open(dst, "w", encoding="utf-8").write(out)
    open(strings_out, "w", encoding="utf-8").write("\n".join(decoded))
    print(f"wrote {dst} ({len(out)} chars)", file=sys.stderr)


def _safe(f) -> bool:
    try:
        return bool(f())
    except Exception:
        return False


if __name__ == "__main__":
    main()
