"""Decode an IllumiPC config file (device list, key tables, layouts).

Files under https://www.illumipc.com/config/ are either plain JSON or
`WMX1:` + base64(XOR("witmod-config-0x5a")). Useful files:

    config/device.json                     every supported keyboard (vid, pid, product)
    config/keys.json                       assignable keys and actions
    config/keys/<product>.json             physical layout (index, label, x, y, w, h)
    config/switch.json                     switch types

This keyboard's product is 7153USHEXYXACPARGB.

Usage:  python -I tools/decode_config.py <file> [out.json]
"""

import base64
import json
import sys

MAGIC = "WMX1:"
KEY = b"witmod-config-0x5a"


def decode(text: str):
    if text.startswith(MAGIC):
        raw = base64.b64decode(text[len(MAGIC) :].strip())
        text = bytes(b ^ KEY[i % len(KEY)] for i, b in enumerate(raw)).decode("utf-8")
    return json.loads(text)


if __name__ == "__main__":
    data = decode(open(sys.argv[1], encoding="utf-8").read())
    out = json.dumps(data, indent=1, ensure_ascii=False)
    if len(sys.argv) > 2:
        open(sys.argv[2], "w", encoding="utf-8").write(out)
    else:
        print(out)
