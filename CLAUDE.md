# CLAUDE.md: Fizz control

Local, offline replacement for the IllumiPC web configurator, for Gert's **Redragon Fizz Magnetic
Plus**. Rust + egui, Windows first, Linux second. Public repo: https://github.com/Gert-Steyn/fizzctl
(`main`). Read this file first; update it whenever something important is learned or the roadmap
moves. `PROTOCOL.md` is the full protocol reference; this file holds context, rules and plans.

## The keyboard

- **Wired only** (USB). No battery, no wireless or tri-mode: don't build UI for those.
- VID `2E3C`, PID `C365` (normal mode). `C366`/`C367` are gamepad modes, `C368` also listed.
- Config channel: interface `MI_02`, usage page `0xFF1B`, usage `0x91`. Windows opens it with no
  driver; Linux needs `linux/99-fizz.rules`.
- Product code `7153USHEXYXACPARGB`, firmware `3.17.01` (built Oct 22 2025), US 60% layout, 61 keys.
- Supports: per-key RGB, polling rate (currently 8000 Hz), key calibration (calibrated). Its status
  does **not** set the RS/RKRT/DKS/dead-band/music flags, but actuation and rapid trigger via
  `0x21 / 0x18` **do** work (those flags are other features).
- **Feature survey (read-only, 2026-11-24, `fizz-probe --survey`):** SOCD supported (status
  "PRCS" flag = SOCD; one pair A+D last-input, off, toggle reported as Fn+Right Shift); macros
  supported (slot 0 empty, reads as `FF` erased flash); advanced keys answer with 40 empty slots
  and dead zones answer all-zero, but their status flags are off, so unproven; one switch type
  only (nothing to configure); profiles: **no reply** (unsupported); Win/Alt locks: flag off.
- Actuation range 0.10–3.40 mm in 0.01 mm steps (config reply `54 01 01 01 0A`). Stock setting on
  all 61 keys: actuation 1.70 mm, rapid trigger on (mode `0x0C`), sensitivity 0.51 mm.
- **No side light strip**: the side-light mode list (`0x02`) is empty. Only key lighting is shown.
- 14 lighting effects: Wave, Breathing, Neon, Twinkle, Radar, Static, Auto ripple, Striation,
  Reactive, Laser, Cross, Action response, Drip, Ripple. Brightness and speed 0–4.
- Stock key map: default layer = standard keys, Fn key stored as `01 FA`. Fn layer is all
  `00 00 00 00` ("built-in Fn action") except `\|` (slot 58) = Print Screen.

## Commands

```powershell
cargo run --release                          # the app
cargo run --bin fizz-probe                   # read-only diagnostics, prints every packet
cargo run --bin fizz-probe -- --rewrite-keymap   # backs up both layers, writes them back unchanged, verifies
cargo run --bin fizz-probe -- --rewrite-triggers # same for every key's actuation settings
cargo run --bin fizz-probe -- --rewrite-poll-rate # writes the current polling rate back, verifies
cargo run --bin fizz-probe -- --rewrite-colours [--layer 0|1]  # NOT a no-op: activates the layer and the Custom effect
cargo run --bin fizz-probe -- --apply-light 2 4 4 FF0000 000000 2 1   # set lighting: mode bri spd fg bg dir rainbow
cargo run --bin fizz-probe -- --rewrite-socd     # SOCD pairs + on/off back unchanged, verifies
cargo run --bin fizz-probe -- --survey           # read-only check of features not built yet
cargo run --bin fizz-probe -- --travel-test 3 --key 22 --allow-blocked  # live travel test on one key; only for agreed experiments
cargo test                                   # unit tests (protocol + keymap)
cargo clippy --all-targets                   # 1 expected warning, in src/graphite.rs (see below)
```

Dev-only env vars for screenshots (not persisted): `FIZZ_EFFECT=custom` (preview the Custom
effect on the Lighting page), `FIZZ_PAGE=keys|actuation|lighting|packets|settings`,
`FIZZ_THEME=dark|light`, `FIZZ_KEY=<slot>` (e.g. 66 = Caps, 22 = Esc; key mapping and actuation),
`FIZZ_LAYER=fn`.

Screenshot the running app: `powershell -File tools/screenshot.ps1 -Title "Fizz control" -Out x.png`.
Launch with `Start-Process`, wait ~4–5 s for it to read the keyboard, capture, `Stop-Process`.

## Code layout

| Path | What |
|---|---|
| `src/protocol.rs` | Pure packet builders/parsers, no I/O. Unit-tested. Blocked-command list lives here. |
| `src/device.rs` | `Keyboard`: open by VID/PID/usage page, send/recv, `request`, `request_pages`, `read_keymap`, `write_keymap` (acks + storing wait + read-back), `wait_until_stored`, `read_trigger_config`, `read_key_triggers`, `write_key_triggers` (grouped + read-back). |
| `src/keymap.rs` | Key catalogue (English names, wire order) and the 61-key `LAYOUT` with default actions. |
| `src/worker.rs` | Background thread owning the connection: reconnect every 1 s, muted status heartbeat every 2 s, `Request` in / `Update` out, `read_snapshot`. |
| `src/app.rs` | Window shell (sidebar, toolbar), Overview, Lighting, Packet log, Settings pages. |
| `src/keymap_page.rs` | Key mapping page: layer switch, picker, save. |
| `src/actuation_page.rs` | Actuation page: multi-select keys, actuation point, rapid trigger, save. |
| `src/socd_page.rs` | Snap tap (SOCD) page: on/off (immediate), key pairs with modes, save. |
| `src/keyboard_view.rs` | Shared clickable keyboard drawing (`KeyFace` per key, Ctrl/Shift-click to extend). |
| `src/graphite.rs` | Copy of the Graphite egui theme module plus local additions (see Design). |
| `src/bin/fizz-probe.rs` | CLI diagnostics and the key-map write-back test. |
| `tools/decode_app.py` | Decrypt + de-obfuscate the vendor `app.js` statically. |
| `tools/decode_config.py` | Decode vendor `config/*.json` (`WMX1:` format). |
| `tools/decode_i18n.py` | Extract the app's built-in (Chinese) UI strings (`witmod-i18n-0x7f`). |
| `tools/screenshot.ps1` | Capture a window to PNG. |
| `backups/` | Key-map backups from `--rewrite-keymap` (git-ignored). |

## Rules

### Safety with the real keyboard
- **Never send** `0x0B` (enters the firmware bootloader), `0x14 02` (the vendor app builds the
  same bytes for "factory reset" and "read sleep timer"), or `0x21 .. 18 02` (starts the live travel
  test, which **made the keyboard drop off USB and reset** on 2026-11-24, see below).
  `protocol::cmd::is_blocked` enforces this in `Keyboard::send`; keep it that way and add any newly
  found dangerous commands there. Unblock something only for a retry Gert has agreed to.
- New command → try it **read-only first** via `fizz-probe`, then decode, then write.
- Before testing a write path on the real keyboard: back up what it holds, then do a **no-op write**
  (write back exactly what was read) and verify with a read-back, like `--rewrite-keymap`. Ask Gert
  before writes that change behaviour.
- Writes keep unknown bytes as-is (read-modify-write). Never write a layer or config assembled
  from a partial read (`parse_keymap` returns `None` on missing pages for this reason).
- Close the GUI before running the probe: both open the same HID interface.

### Design (Graphite, via the `unified-ui` skill)
- Use the `unified-ui` skill for any UI work. Tokens only: no hex colours, font sizes, radii or
  spacing literals in screen code (`app.rs`, `keymap_page.rs`). Check with
  `grep -nE 'Color32::from|from_rgb|add_space\([0-9]|\.size\([0-9]|CornerRadius::same|Margin::' src/app.rs src/keymap_page.rs`.
- One primary (accent) button per view. Sentence case, verb-first buttons. Lucide icons only.
- `setting_row` uses `Sides::shrink_left().wrap()`: the control is laid out first and a long
  description wraps instead of widening the card (a long one used to push cards off-screen).
- New reusable components go in `src/graphite.rs`, not in screens. Added so far (candidates to fold
  back into the skill, tell Gert): `segmented`, `setting_row`, `value_text`, `banner`.
- The one clippy warning (`field_reassign_with_default`) is in the skill's own `graphite.rs`; leave it
  unless fixing it upstream in the skill too.
- Always look at the result (screenshots in dark **and** light) before calling UI work done.

### Repo
- Public repo. **Don't commit** the saved vendor web page (`Online Keyboard Driver*`, git-ignored),
  decoded vendor JS, `backups/`, or anything with the keyboard's serial number.
- `feeders-2026-11-24.yaml` in the project root is an unrelated TVM802 pick-and-place export. Gert
  knows; leave it untracked and untouched.
- Commit only when Gert asks. Commit messages end with the Co-Authored-By line.

## egui 0.37 gotchas (learned the hard way)
- `ui.set_max_width(w)` **sets** the width, it doesn't cap it. Use
  `ui.set_max_width(ui.available_width().min(w))`.
- `ui.with_layout(..)` claims all remaining height. For left/right rows use `egui::Sides`
  (centres both sides; row height = taller side). `setting_row` uses it.
- `ui.horizontal` inherits right-to-left from its parent: inside `Sides`' right side, items come out
  reversed. `segmented` compensates by reversing when `ui.layout().prefer_right_to_left()`.
- `ui.vertical` inside a centred row pins to the top; add a lone label directly to centre it.
- `Ui::id()` is deprecated → `scope_id()`. App trait is `fn ui(&mut self, ui, frame)`; panels are
  `egui::Panel::left/top(..).show(ui, ..)`.
- Give each page's `ScrollArea` an `id_salt`, or pages share (and persist) one scroll offset.
- `Label::wrap()` to wrap long text (the packet log's hex lines).

## Tooling gotchas
- PowerShell variables are **case-insensitive**: `$s` overwrites `$S`.
- Bash heredocs through the tool can mangle backslashes (`'\\'`, `\n`). For scripts with
  backslashes, write a file with the Write tool and run it instead of inlining.
- Reading the vendor JS: never execute it; `tools/decode_app.py` works statically. Run Python on
  downloaded files with `python -I`.
- hidapi on Windows prefixes input reports with the report ID (64 bytes); `Keyboard::recv` strips it.

## Protocol essentials (full detail in PROTOCOL.md)
- Report ID `0x01`, 63 data bytes, both ways. `[cmd, sub, pageHi, pageLo, len, payload…]`.
  No checksum, no handshake. Bytes after `len` are stale: always honour `len`.
- Status `0x01`; product info `0x0D` (2 pages: identity CSV, build date); serial `0x15`;
  poll rate `0x21 [4]=1 [5]=9` → `[6]` 1/2/4/8 = 1/2/4/8 kHz; set = `0x21 [5]=9 [6]=1 [7]=rate`.
- Lighting `0x07` (keys) / `0x08` (side): read `[1]=1`; write `[4]=0x0E` + mode, brightness, speed,
  fg RGB, bg RGB, direction, rainbow, power, timer. Mode list `0x0A`/`0x02`, trailer `A5 5A spd bri`.
  Mode numbers index the vendor preview table (10 = Custom, 100 = Music).
- Key map `0x18`: read `[1]=0x80/0x82`, write `[1]=0x00/0x02`, 132 slots × 4 bytes, 10 pages of 56.
  Slot `[kind, code, arg1, arg2]`; kinds in `keymap::kind`. Mouse (`0x12`) is byte-swapped on the wire.
  **Write verified on the real keyboard 2026-11-24**: 10/10 pages acked, read-back identical,
  storing flag never seen (save finishes within the 250 ms settle).
- Actuation `0x21 [4]=0x18`: config `[5]=4` → `[6]|[9]<<8` max steps, `[7]` unit (÷100, or ÷1000 if
  high-precision), `[8]` RT unit, `[10]` min steps. Per key `[5]=5 [6]=row [7]=col` → mode `[6]`,
  travel `[7]|[11]<<8`, press `[9]|[13]<<8`, release `[10]|[14]<<8`, echoes row/col at `[15]`/`[16]`.
  Write: `[5]=mode`, 22-byte column bitmap at `[6..28]` (bit = row), then `t t p r` low bytes and
  `t t p r` high bytes at `[28..36]`. One packet sets any number of keys; no ack, so read back.
  Modes: `0` fixed, `0x0C` RT single sensitivity (release = press), `0x0D` RT split, `0x01` legacy RT.
  **No-op write verified 2026-11-24** (1 packet for all 61 keys, read-back identical).
- SOCD `0x24`: pairs read `[1]=1` (2 pages × 10 slots of `state mode key1 key2`; state 0 or
  `07 FF FF FF` = empty), write `[1]=0` same layout (acked `[24 00]`); on/off read `[1]=2` →
  payload `[0]` on, `[4] [5]` = toggle keys (`FA E5`); set `[1]=3 [4]=1 [5]=on`. Modes: 0 last
  input wins, 1 key 1 priority (vendor default), 2 key 2 priority, 3 neutral. No-op write of pairs
  and on/off verified 2026-11-24; writing pairs does **not** turn SOCD on, and touched nothing else.
- Polling rate write `0x21 [5]=9 [6]=1 [7]=1|2|4|8` (no reply; read back). No-op write verified
  2026-11-24 (8000 → 8000).
- **Live travel test incident (2026-11-24):** `0x21 [4]=0x18 [5]=2` + all 61 keys in the bitmap
  (byte-identical to the vendor app's `openTriggerTest`) made the keyboard stream ~5 reports
  (`21 00 00 00 03 01 row col lo hi`, idle keys at 0–16 steps), then **drop off USB**: Windows logged
  "surprise removed as it is reported as missing on the bus" (Kernel-PnP 1010). It re-enumerated by
  itself; all settings were intact afterwards (checked read-only). Stop (`[5]=3`) could not be sent.
  Not yet known whether it's the full bitmap, a missing host heartbeat, or a firmware bug.
- **Per-key colours `0x09` (2026-11-24):** read `[1]=0x80|layer`, 8 pages × 18 RGB (slot order),
  write `[1]=layer` (acked `[09, layer]`), active layer `[1]=0x20` → reply `[1]`. Both layers were
  all black. **A colour write is not a no-op**: writing a layer makes it the active layer
  (confirmed: writing layer 0 switched active 1 → 0) and, per the first test, switches the
  lighting to the Custom effect (mode 10) with Custom's *own stored* brightness/direction (it had
  brightness 0, so the backlight went dark). The firmware keeps settings per effect. So the app
  always sends the lighting config right after any colour save. After the incident the lighting
  was restored with `fizz-probe --apply-light 2 4 4 FF0000 000000 2 1` (Gert agreed) and verified;
  that was also the first real lighting write and it read back exactly.
- **One-key retry (2026-11-24, agreed with Gert):** Esc only, 3 s, keys idle. The keyboard acked the
  start (`21 00 00 00 01 00`), sent no travel reports, accepted stop, kept answering, and Windows
  logged no removal. So a single-key test is stable *while idle*; the first run's reset came either
  from the full bitmap or from the report stream itself.
- **One-key test with presses (2026-11-24):** Right Ctrl (slot 122), 20 s, Gert pressing it:
  416 reports, travel 0..340 steps (= the full 0–3.40 mm), stop accepted, no USB removal. Reports
  are `21 00 00 00 03|05 01 row col lo hi …`, at a high rate while the key moves. So the stream
  itself is fine; the first reset most likely came from the **61-key bitmap**. (An earlier attempt
  with Esc was aborted because Esc interrupts Claude, see memory; still no disconnect.)
  `fizz-probe --travel-stop` sends only the stop, for cleanup after an interrupted test.
- Vendor app sources: `https://www.illumipc.com/app.js` (`WMJ1` + XOR `witmod-app-0x5a`);
  configs under `/config/` (`WMX1:` base64 XOR `witmod-config-0x5a`). Layout file for this board:
  `config/keys/7153USHEXYXACPARGB.json`.
- Finding things in the decoded JS: every command packet is built with `new Uint8Array(0x3f)`;
  the input dispatcher is the class with `["handle"](…)` and `switch(aFN){case 0x1:…`; command
  constants are a `const adw=0x1,adx=0x2,…` line. Names change between vendor builds.

## Status

| Feature | State |
|---|---|
| Device overview (name, firmware, build date, product code, serial, poll rate, calibration, features) | Done, read |
| Key lighting (effect, brightness, speed, rainbow, colours, direction) | Done. Read and write verified (probe `--apply-light`, 2026-11-24); the GUI's Apply not yet tried by Gert |
| Key mapping, default + Fn layers | Done. Read verified; write path verified with a no-op write. A real remap via the GUI's Save button not yet tried |
| Actuation + rapid trigger (per key, multi-select) | Done. Read verified; write path verified with a no-op write. A real change via the GUI not yet tried |
| Polling rate (1/2/4/8 kHz on Overview, applies immediately) | Done. No-op write verified; a real change via the GUI not yet tried |
| Per-key colours (Custom effect: paint keys on the Lighting page) | Done. Read verified; colour write verified (acks + read-back); lighting write verified via probe. Painting from the GUI not yet tried |
| Snap tap (SOCD): on/off, up to 20 pairs with 4 modes | Done. Read verified; no-op writes verified; a real change via the GUI not yet tried |
| Live travel test | **Blocked** in the app: a 61-key start reset the keyboard; one key works (probe only) |
| Packet log | Done |
| Theme setting (System/Dark/Light, persisted) | Done |
| Hot-plug reconnect / unplug detection | Built; unplug detection untested |
| Linux | Builds should work via hidapi + udev rule; never run on Linux |

## Roadmap (in order)

1. ~~Actuation and rapid trigger~~ done 2026-11-24. Follow-ups, in order:
   - **Live travel test**: blocked after a 61-key start reset the keyboard. One key is stable,
     including while pressed (0–340 steps). Next, each with Gert's OK and a non-Esc key: a small
     bitmap (e.g. 5 keys), then larger ones, to find what's safe; maybe the vendor's `0x01`
     heartbeat matters for the full set. If only small sets are safe, the UI can test the
     selected keys only. The worker must stream input (no `drain()`/heartbeat while testing) and
     always send `[5]=3` on stop, page change and shutdown. If it works: show a live
     bar per key on the drawing; the worker must stream input (no `drain()`/heartbeat while testing)
     and always send `[5]=3` on stop, page change and shutdown.
   - **Calibration**: `[5]=8 [6]=0/1` start/stop all keys, `[5]=7 row col` one key, `[5]=0x0F/0x10`
     "any key"; progress arrives as `0x21 [5]=7/8/0x0F` with `[6]` 0 success, 1 progress (bitmap at
     `[7..0x1D]`), 2/4 failed. Treat as behaviour-changing: ask Gert first.
   - Reset to defaults `[5]=6` (unknown defaults; ask first). Presets/quick buttons (e.g. "Gaming 0.4 mm").
2. ~~Polling rate setting~~ done 2026-11-24.
3. ~~Per-key RGB~~ done 2026-11-24 (edits the active colour layer only). Possible follow-ups:
   switch between the two colour layers (writing a layer activates it), drag-to-paint, presets.
4. ~~SOCD~~ done 2026-11-24 (not originally on the list; found by the survey).
5. **Macros.** `0x19`: read slot `[1]=0x80+slot`, write `[1]=slot` 58-byte pages (256-byte buffer),
   delete `[1]=3 [4]=index`. Step = `{keyCode, type|state<<4, delayHi, delayLo}`. Key-map kind `0x10`
   with `arg1` = macro index. Needs a recorder UI.
6. Then: calibration (ask first); dead zones and advanced keys only after a test proves the
   firmware uses them; export/import of all settings; restore a `backups/` file. Skip: profiles (no
   reply), locks (flag off), switch types (one type), sleep timer (blocked command).
7. Release (after the features, per Gert): app icon, single `.exe`, GitHub Actions building Windows + Linux binaries; run on Linux once.

## Ideas / open questions
- What do light-config `power` and `timer` bytes mean? Passed through unchanged for now.
- Does `0x14 02` mean factory reset or sleep-timer read on this firmware? Keep blocked until proven.
- Fn layer "built-in actions": which Fn combos does the firmware provide (Fn+1..= → F1..F12?)? Could
  be listed in the UI once known (observe with the packet log / key tester).
- Import/export of a full profile (JSON) like the vendor's `exportProfile`.

## Gert's preferences
- Rust, cross-platform (Windows first, Linux next), apps styled with his Graphite system.
- Wants the real keyboard tested where safe; ask before behaviour-changing writes.
- Keep this file current: record decisions, findings and roadmap changes here.
