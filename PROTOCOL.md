# Redragon Fizz Magnetic Plus – HID config protocol

Reverse-engineered from the IllumiPC web configurator (`https://www.illumipc.com/`, `app.js`,
XOR-encrypted with key `witmod-app-0x5a` after a 4-byte `WMJ1` header, then string-array
obfuscated). Confirmed against the keyboard connected to this PC on 2026-11-24.

## Device

| Field       | Value                                   |
|-------------|-----------------------------------------|
| Vendor ID   | `0x2E3C`                                |
| Product ID  | `0xC365` (also `0xC366` = gamepad classic mode, `0xC367` = Xbox360 mode, `0xC368`…) |
| Interface   | `MI_02` – "HID-compliant vendor-defined device" |
| Usage page  | `0xFF1B`                                |
| Usage       | `0x91`                                  |

Open the HID interface whose usage page is `0xFF1B` (don't use the keyboard or mouse collections).

## Framing

* **Output report ID `0x01`**, 63 data bytes (64 total with report ID).
* **Input report ID `0x01`**, 63 data bytes. Ignore any other report ID.
* No checksum and no handshake/auth. The web app's device "allowlist" only runs in the browser.

Common packet layout (both directions):

```
byte 0   command
byte 1   sub-command / layer / slot (command-specific)
byte 2-3 page index, big-endian (for multi-packet transfers)
byte 4   payload length
byte 5.. payload (max 58 bytes)
```

Multi-page reads: send one request, and the device streams pages back. The reply is complete
when the page index (bytes 2-3) reaches the command's last page.

## Commands

| Cmd    | Name                | Request                                              | Reply / notes |
|--------|---------------------|------------------------------------------------------|---------------|
| `0x01` | Device status / heartbeat | all zero                                        | `[1]` mode, `[8]` battery status, `[9]` battery %, `[10]` tri-mode; `[5..0x17]` = support flags (see below). The app sends this as a heartbeat. |
| `0x02` | Side-light mode list | `[1]=0`                                              | payload = mode ids…, then maxSpeed, maxBrightness (last 2 of the final 4 bytes) |
| `0x07` | Main light config   | **read:** `[1]=1`. **write:** `[1..3]=0, [4]=0x0E`, payload below | |
| `0x08` | Side light config   | same as `0x07`                                       | |
| `0x09` | Per-key custom RGB  | read: `[1]=0x80` (layer 0) / `0x81` (layer 1). `[1]=0x20` = get current layer. write: `[1]=layer`, pages 0..7, 54 bytes each (18 keys × RGB) | ack `[0]=9,[1]=layer` |
| `0x0A` | Main light mode list | `[1]=0`                                             | like `0x02` |
| `0x0B` | Enter bootloader    | –                                                    | ⚠ **do not send**: flashing mode |
| `0x0D` | Product info        | –                                                    | ASCII CSV in payload; field 4 = product, field 5 = firmware version |
| `0x0E` | Fn-editable keys    | –                                                    | bitmap per column, 6 rows; key index = `22*row + col` |
| `0x14` | Device settings     | `[1]=0,[4]=1`: `[0x0C]` lock Win, `[0x0D]` lock Shift+Tab, `[0x0E]` lock Alt+F4, `[0x0F]` lock Alt+Tab, `[0x11]` super-performance, `[0x12]` 6KRO | |
|        | Reset keyboard      | `[1]=0,[4]=1,[5]=1`                                  | |
|        | Factory reset       | `[1]=2` (no payload) | ⚠ the web app's code builds **byte-identical** packets for factory reset and "get sleep timer". Treat `0x14 [1]=2` as dangerous until tested. |
|        | Sleep timer get     | `[1]=2`                                              | `[8..9]` sleep s, `[0x0A..0x0B]` standby s (BE) |
|        | Sleep timer set     | `[1]=1`, `[8..9]` sleep s, `[0x0A..0x0B]` standby s  | |
|        | Gamepad mode        | `[1]=3,[4]=1,[5]=mode`                               | |
| `0x15` | Serial number       | –                                                    | ASCII payload |
| `0x17` | Music-rhythm light  | `[4]=2,[5]=on,[6]=on`                                | |
| `0x18` | Key mapping         | read: `[1]=0x80` default / `0x82` Fn; pages 0..9. write: `[1]=0` default / layer, 56 bytes per page | read: 4 bytes per key `{code1, hidCode, code3, code4}`, 14 keys/page |
| `0x19` | Macros              | read slot: `[1]=0x80+slot`; write: `[1]=slot`, 58-byte pages; delete: `[1]=3,[4]=index` | |
| `0x21` | Profiles / trigger / poll rate | see below                                 | |
| `0x22` | Advanced keys (DKS/MT/TGL/RS/RKRT/MDS) | read `[1]=0x80`; write `[1]=slot(0..39),[4]=0x32` | |
| `0x23` | Live ADC stream     | `[5]=1` open / `[5]=0` close                         | streams key travel values |
| `0x24` | SOCD                | `[1]=1` read data, `[1]=2` read power, `[1]=3,[4]=1,[5]=on` set power, `[1]=0` write pages | |
| `0x25` | Switch type per key | `[1]=0` list, `[1]=2` read per-key, `[1]=1` write    | |
| `0x26` | Dead band           | `[1]=0` read (pages), `[1]=1` write (58-byte pages, 2 bytes/key: top, bottom) | |

### `0x21` sub-commands

| Packet                          | Meaning |
|---------------------------------|---------|
| `[4]=1,[5]=9`                   | read poll rate → reply `[6]` = rate index |
| `[5]=9,[6]=1,[7]=rate`          | set poll rate |
| `[5]=0x11`                      | read active profile (reply event `presetIndex`) |
| `[5]=0x12,[6]=index`            | set active profile |
| `[4]=0x18,[5]=4`                | read global trigger config |
| `[4]=0x18,[5]=5,[6]=row,[7]=col`| read one key's trigger (actuation / RT) data |
| `[4]=0x18, payload@5`           | write trigger data |
| `[4]=0x18,[5]=6`                | reset triggers |
| `[4]=0x18,[5]=2, bitmap@6`      | start trigger test |
| `[4]=0x18,[5]=3`                | stop trigger test |
| `[4]=0x18,[5]=7,[6]=row,[7]=col`| calibrate one key |
| `[4]=0x18,[5]=8,[6]=0/1`        | start/stop calibrate all keys |
| `[4]=0x18,[5]=0x0F` / `0x10`    | start/stop "any key" calibration |

Key index convention: `index = 22*row + col` (22 columns, 6 rows, max 132 keys).

### Light config payload (`0x07` / `0x08`)

| Offset | Field |
|--------|-------|
| 5      | mode |
| 6      | brightness |
| 7      | speed |
| 8-10   | foreground R,G,B |
| 11-13  | background R,G,B |
| 14     | direction (0-5) |
| 15     | fullColor (rainbow flag) |
| 16     | power (on/off) |
| 17     | timer |

### Support flags (status reply, payload = bytes 5..0x17, index `s[n]`)

* `s[0]==1` lock-key supported, `s[9]==1` PRCS, `s[10]` super-performance supported, `s[11]` super-performance on
* `s[12]` bits: 0 any-key calibration, 1 custom light, 2 more switches, 3 poll rate, 4 dead band, 5 music rhythm, 6 report key settings, 7 all/6-key switch
* `s[13]` bits: 0 high precision, 1 light position switch, 2 gamepad, 3 RS, 4 RKRT, 5 DKS mouse, 6 MDS
* `s[14]==1` calibrated, `s[16]==1` music rhythm on, `s[17]` bit0 6-key mode, bit1 storing data (flash write in progress)

After writing key maps, poll `0x01` until the "storing" bit (`s[17]` bit 1, i.e. reply byte `0x16` bit 1) clears.

## Confirmed on the real keyboard (firmware 3.17.01, 2026-11-24)

* Report ID `0x01` in and out, 63 data bytes. Replies echo the command byte.
* `0x0D` product info replies with **two pages**: page 0 `W669,34,KB,FR,7153USHEXYXACPARGB,V3.17.01`,
  page 1 the build date `Oct 22 2025,16:24:10`. Text ends at the first NUL; stale bytes follow it.
* `0x15` serial: 32 hex characters.
* `0x21 [4]=1 [5]=9` polling rate reply `[6]=8` → 8000 Hz.
* `0x0A` key-light mode list: `02 01 03 09 04 00 0E 0F 06 07 0B 0C 10 08`, then trailer `A5 5A 04 04`
  (max speed 4, max brightness 4).
* `0x02` side-light mode list is empty (trailer only): **this keyboard has no side light**.
* `0x07 [1]=1` light config reply has length 12 (no timer byte): `02 04 03 FF 00 00 00 00 00 00 01 00`
  = Wave, brightness 4, speed 3, red/black, direction 0, rainbow on.
* Status support bytes: `s[12]=0x0B` (calibration, per-key RGB, polling rate), `s[14]=1` calibrated.

## Key mapping (`0x18`), confirmed by reading the real keyboard

* A layer has 132 slots (6 rows × 22 columns, slot = `22*row + col`), 4 bytes each = 528 bytes.
* **Read:** `[1]=0x80` default layer, `0x82` Fn layer. The keyboard answers with pages 0–9, 14 slots
  (56 bytes) per page; page 9 carries 24 bytes. Bytes after the payload length are stale and must be ignored.
* **Write:** `[1]=0x00` default, `0x02` Fn, pages 0–9 of up to 56 bytes, the whole layer every time.
  The vendor app waits for an ack `[0]=0x18, [1]=0 or layer` per page, then polls status until
  the "storing" flag (status reply byte `0x16` bit 1) clears.
* **Write confirmed 2026-11-24** by writing both layers back unchanged (`fizz-probe --rewrite-keymap`):
  all 10 pages of each layer were acknowledged, the read-back matched byte for byte, and the other
  layer was untouched. The "storing" flag was never observed; the save finishes within the 250 ms settle wait.
* Slot bytes `[kind, code, arg1, arg2]`:

  | kind | Meaning | Example |
  |------|---------|---------|
  | `00` | HID key, `code` = usage | `00 29` Esc |
  | `01` | Fn | `01 FA` |
  | `02`–`06` | Lighting controls (effect, brightness, speed, direction, colour) | `03 00` brightness up |
  | `07` | Media / consumer | `07 13` play/pause |
  | `10` | Macro, `arg1` = macro index | |
  | `11` | Locks / lights | `11 00` lock keyboard, `11 02` lock Win, `11 04` lights on/off |
  | `12` | Mouse, wire order `[12, arg, button, 0]` | `12 01 FF 00` scroll down |
  | `13` | Shortcut `[13, modifier, key, 0]` | `13 E3 07` Win+D |
  | `17` | Advanced key (DKS/MT/…) | |
  | `18`/`19` | Gamepad buttons | |

* In the Fn layer, `00 00 00 00` means "use the firmware's built-in Fn function". Stock Fn layer on
  this keyboard: only `\|` (slot 58) is set, to Print Screen (`00 46`).
* Layout and key names come from `https://www.illumipc.com/config/keys/7153USHEXYXACPARGB.json` and
  `config/keys.json` (plain JSON; files prefixed `WMX1:` are base64 XOR `witmod-config-0x5a`).

## Actuation and rapid trigger (`0x21`, `[4]=0x18`), confirmed on the real keyboard

* **Config** `[5]=4` → reply `21 00 00 00 06 04 54 01 01 01 0A`: max steps `[6] | [9]<<8` = 340,
  unit `[7]` = 1 → 0.01 mm (÷100; ÷1000 when the status high-precision flag is set; 0 means 10),
  RT unit `[8]` = 0.01 mm, min steps `[10]` = 10. So actuation is 0.10–3.40 mm in 0.01 mm steps.
* **Per key** `[5]=5 [6]=row [7]=col` → `[6]` mode, travel `[7] | [11]<<8`, press `[9] | [13]<<8`,
  release `[10] | [14]<<8`, and the reply echoes row/col at `[15]`/`[16]`. `[8]` and `[12]` repeat travel.
  Stock on every key: `0C AA AA 33 33` = rapid trigger, 1.70 mm, 0.51 mm / 0.51 mm.
* **Write** (no reply): `[5]` = mode, `[6..28]` = 22 column bytes, bit *n* = row *n* selects the keys,
  `[28..36]` = travel lo, travel lo, press lo, release lo, travel hi, travel hi, press hi, release hi.
  One packet applies one setting to any set of keys. Verified 2026-11-24 by writing every key's
  settings back unchanged (`fizz-probe --rewrite-triggers`): one packet, all 61 keys read back identical.
* **Modes**: `0x00` fixed actuation; `0x0C` rapid trigger with one sensitivity (vendor app sets
  release = press); `0x0D` rapid trigger with separate press/release; `0x01` older rapid trigger.
* Not built yet: `[5]=6` reset, `[5]=2/3` live travel test (pushes `0x21 [5]=1` row, col, travel),
  calibration `[5]=7/8/0x0F/0x10` (pushes progress as `0x21 [5]=7/8/0x0F`).

## Safe first tests

Read-only, no side effects: `0x01` status, `0x0D` product info, `0x15` serial, `0x07 [1]=1` read lighting.
