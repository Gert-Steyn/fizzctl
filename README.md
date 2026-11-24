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

Test the save paths without changing anything. Each command writes what the
keyboard holds back unchanged and checks it still holds the same values (key
map, actuation, polling rate). The first two also save a backup to `backups/`:

```powershell
cargo run --bin fizz-probe -- --rewrite-keymap
cargo run --bin fizz-probe -- --rewrite-triggers
cargo run --bin fizz-probe -- --rewrite-poll-rate
cargo run --bin fizz-probe -- --rewrite-socd
cargo run --bin fizz-probe -- --profile-roundtrip
```

## What works

| Area | Status |
|---|---|
| Device info: name, firmware, build date, product code, serial | Read |
| Polling rate (1/2/4/8 kHz) | Read and write |
| Calibration state, supported features | Read |
| Key lighting: effect, brightness, speed, rainbow, colours, direction | Read and write |
| Per-key colours (Custom effect): paint keys on the keyboard drawing | Read and write |
| Key mapping, default and Fn layers: keys, media, mouse, shortcuts, lighting controls | Read and write |
| Actuation point and rapid trigger, per key or many keys at once | Read and write |
| Snap tap (SOCD): on/off and up to 20 key pairs with 4 modes | Read and write |
| Macros: 10 macros of key presses and clicks with delays, 4 play modes, assigned to keys via Key mapping; record steps by typing | Read and write |
| Backup and restore of every setting to a file, with an automatic safety backup before a restore | Yes |
| Packet log of all traffic | Yes |
| Calibration | Not built yet |
| Live travel test | Blocked: starting it made the keyboard reset (see `PROTOCOL.md`) |

### Actuation

Every key shows its actuation point in mm; accent numbers have rapid trigger on.
Click a key, Ctrl-click several, or **Select all**, then set the actuation
point, rapid trigger, and its sensitivity (one value, or separate press and
release). Only the settings you change are applied to the selected keys.
**Save actuation** sends one packet per distinct setting and reads every
changed key back to check it.

### Backup and restore

Settings, **Backup and restore**: **Save settings** reads every setting on the
keyboard (key maps, lighting, key colours, actuation, snap tap, macros and
polling rate) and saves it as a JSON file in the app's data folder; **Open
folder** shows where. **Restore** next to a file writes it back after a
confirmation. Before anything is written, the current settings are saved as a
"before a restore" file, so a restore can always be undone, and each section is
read back afterwards to check it. Files hold the keyboard's product code and
firmware but no serial number, so they are safe to share, and the app warns if
a file came from a different keyboard model.

### Macros

Pick a macro slot, create it, then add steps by hand or press **Record keys**
and type: every key you press in this window is added to the end of the
steps, with the pauses you make (or a fixed delay, your choice). Shift, Ctrl
and Alt are recorded as the left or right key you used, and any key still held
when you stop is released for you. Recording ends when you press **Stop
recording**, pick another macro or page, switch to another window, or reach 40
steps. The Windows key can't be recorded; use **Add key press** for it.
**Save macros** writes all 10 macros at once.

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
- `src/profile.rs` and `src/backup.rs`: the settings file format (read, validate, apply) and the Settings page section for it
- `src/recorder.rs` and `src/capture.rs`: macro recording (event-to-step logic, and egui keys to HID codes)
- `src/keymap.rs`: the key catalogue (English names) and this keyboard's 61-key layout
- `src/app.rs`: the window; `src/keymap_page.rs`: the key mapping page; `src/graphite.rs`: the shared theme module
- `PROTOCOL.md`: the reverse-engineered protocol
- `tools/`: decoders for the vendor's web app and config files, and a window screenshot script
- `CLAUDE.md`: project notes, rules and roadmap

## Safety

`device::Keyboard::send` refuses three commands outright: `0x0B` (enter
firmware bootloader), `0x14 02`, and the live travel test start, which made the
keyboard reset. The vendor app builds the same `0x14 02`
bytes for both "factory reset" and "read sleep timer", so it's blocked until
we know which it does.

## Linux

Install the udev rule once so you don't need root:

```sh
sudo cp linux/99-fizz.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules && sudo udevadm trigger
```

## Developer overrides

`FIZZ_PAGE` (`keys`, `actuation`, `socd`, `macros`, `lighting`, `packets`, `settings`),
`FIZZ_RECORD=1` (start recording into macro 1) picks the start page,
`FIZZ_THEME` (`dark`, `light`) forces a theme without saving it, and on the
key mapping and actuation pages `FIZZ_KEY` (a slot number, e.g. `66` for Caps)
preselects a key and `FIZZ_LAYER=fn` opens the Fn layer. All are for screenshots.
