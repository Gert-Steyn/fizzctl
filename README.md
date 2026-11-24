# Fizz control

A local, offline replacement for the IllumiPC web configurator, for the
**Redragon Fizz Magnetic Plus** (wired, USB `2E3C:C365`). Written in Rust with
egui, styled with the Graphite design system. Works on Windows; Linux support
is built in through `hidapi` (see below).

## Run

```powershell
cargo run --release
```

Read-only connection test that prints every packet:

```powershell
cargo run --bin fizz-probe
```

Test the save paths without changing anything. Each command backs up what the
keyboard holds to `backups/`, writes it back unchanged and checks the keyboard
still holds the same values (key map, then actuation):

```powershell
cargo run --bin fizz-probe -- --rewrite-keymap
cargo run --bin fizz-probe -- --rewrite-triggers
```

## What works

| Area | Status |
|---|---|
| Device info: name, firmware, build date, product code, serial | Read |
| Polling rate, calibration state, supported features | Read |
| Key lighting: effect, brightness, speed, rainbow, colours, direction | Read and write |
| Key mapping, default and Fn layers: keys, media, mouse, shortcuts, lighting controls | Read and write |
| Actuation point and rapid trigger, per key or many keys at once | Read and write |
| Packet log of all traffic | Yes |
| Macros, per-key RGB, live travel test, calibration | Protocol mapped in `PROTOCOL.md`, not built yet |

### Actuation

Every key shows its actuation point in mm; accent numbers have rapid trigger on.
Click a key, Ctrl-click several, or **Select all**, then set the actuation
point, rapid trigger, and its sensitivity (one value, or separate press and
release). Only the settings you change are applied to the selected keys.
**Save actuation** sends one packet per distinct setting and reads every
changed key back to check it.

### Key mapping

Click a key on the drawing, then pick what it should do. Changed keys get a
dot until you press **Save key map**, which writes every changed layer, waits
for the keyboard to finish saving to flash, then reads the layer back to check
it. On the Fn layer, keys left on "Built-in Fn action" keep the firmware's own
Fn shortcuts. If no key is set to Fn, the page warns you, because without one
the Fn layer can't be reached. **Reset key** puts a default-layer key back to
what its keycap says.

The keyboard has no side light strip (its side-light mode list is empty), so
only key lighting is shown.

## Layout

- `src/protocol.rs`: packet builders and parsers, no I/O, unit-tested
- `src/device.rs`: opens the vendor HID interface (usage page `0xFF1B`) and does request/reply
- `src/worker.rs`: background thread that owns the connection, reconnects and sends heartbeats
- `src/keymap.rs`: the key catalogue (English names) and this keyboard's 61-key layout
- `src/app.rs`: the window; `src/keymap_page.rs`: the key mapping page; `src/graphite.rs`: the shared theme module
- `PROTOCOL.md`: the reverse-engineered protocol
- `tools/`: decoders for the vendor's web app and config files, and a window screenshot script
- `CLAUDE.md`: project notes, rules and roadmap

## Safety

`device::Keyboard::send` refuses two commands outright: `0x0B` (enter
firmware bootloader) and `0x14 02`. The vendor app builds the same `0x14 02`
bytes for both "factory reset" and "read sleep timer", so it's blocked until
we know which it does.

## Linux

Install the udev rule once so you don't need root:

```sh
sudo cp linux/99-fizz.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules && sudo udevadm trigger
```

## Developer overrides

`FIZZ_PAGE` (`keys`, `actuation`, `lighting`, `packets`, `settings`) picks the start page,
`FIZZ_THEME` (`dark`, `light`) forces a theme without saving it, and on the
key mapping and actuation pages `FIZZ_KEY` (a slot number, e.g. `66` for Caps)
preselects a key and `FIZZ_LAYER=fn` opens the Fn layer. All are for screenshots.
