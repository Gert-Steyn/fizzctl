"""Extract the IllumiPC app's built-in translations.

The decoded app.js (see decode_app.py) embeds each language as
`<fn>("<base64>")`, where the base64 is XOR "witmod-i18n-0x7f" JSON. This
finds every such blob, decodes it, and writes the language dictionaries to a
JSON file. Optionally prints keys matching a search term from the English one.

Usage:  python -I tools/decode_i18n.py app.deob.js i18n.json [search]
"""

import base64
import json
import re
import sys

KEY = b"witmod-i18n-0x7f"


def decode(blob: str):
    raw = base64.b64decode(blob)
    text = bytes(b ^ KEY[i % len(KEY)] for i, b in enumerate(raw)).decode("utf-8")
    return json.loads(text)


def flatten(d, prefix=""):
    for k, v in d.items():
        path = f"{prefix}.{k}" if prefix else k
        if isinstance(v, dict):
            yield from flatten(v, path)
        else:
            yield path, v


def main() -> None:
    src, out = sys.argv[1], sys.argv[2]
    search = sys.argv[3].lower() if len(sys.argv) > 3 else None
    s = open(src, encoding="utf-8").read()
    decoder = re.search(r'const ([\w$]+)="witmod-i18n-0x7f";function ([\w$]+)\(', s).group(2)
    blobs = re.findall(re.escape(decoder) + r'\("([A-Za-z0-9+/=]{100,})"\)', s)
    langs = []
    for blob in blobs:
        try:
            langs.append(decode(blob))
        except Exception:
            pass
    json.dump(langs, open(out, "w", encoding="utf-8"), ensure_ascii=False, indent=1)
    print(f"decoded {len(langs)} dictionaries", file=sys.stderr)
    if search:
        # Pick the dictionary whose text looks English.
        for d in langs:
            flat = dict(flatten(d))
            sample = " ".join(str(v) for v in list(flat.values())[:200])
            if " the " in sample or " and " in sample:
                for k, v in flat.items():
                    if search in k.lower() or search in str(v).lower():
                        print(f"{k} = {v}")
                break


if __name__ == "__main__":
    main()
