//! Connection test. By default it sends only query commands and prints what
//! the keyboard reports, with every packet in hex.
//!
//! ```text
//! cargo run --bin fizz-probe
//! cargo run --bin fizz-probe -- --rewrite-keymap
//! cargo run --bin fizz-probe -- --rewrite-triggers
//! cargo run --bin fizz-probe -- --rewrite-poll-rate
//! cargo run --bin fizz-probe -- --rewrite-colours [--layer 0|1]
//! cargo run --bin fizz-probe -- --apply-light 2 4 4 FF0000 000000 2 1
//! cargo run --bin fizz-probe -- --travel-test 3 --key 22 --allow-blocked
//! cargo run --bin fizz-probe -- --travel-stop
//! cargo run --bin fizz-probe -- --rewrite-socd
//! cargo run --bin fizz-probe -- --read-macros
//! cargo run --bin fizz-probe -- --macro-write-test
//! cargo run --bin fizz-probe -- --dump-macro <slot>
//! cargo run --bin fizz-probe -- --clear-macro <slot>
//! cargo run --bin fizz-probe -- --profile-export settings.json
//! cargo run --bin fizz-probe -- --profile-restore settings.json
//! cargo run --bin fizz-probe -- --profile-roundtrip
//! cargo run --bin fizz-probe -- --survey      # read-only check of the remaining features
//! ```
//!
//! `--rewrite-keymap` tests the key-map save path without changing anything:
//! it backs up both layers to `backups/`, writes each one back unchanged, and
//! checks that the keyboard still holds exactly the same bytes.
//! `--rewrite-triggers` does the same for every key's actuation settings, and
//! `--rewrite-poll-rate` for the polling rate, `--rewrite-colours` for both
//! per-key colour layers. `--travel-test <seconds>` runs
//! the live travel test (all keys, or one with `--key <slot>`), prints what the
//! keyboard streams, and always stops it. Its start command is blocked because it
//! reset the keyboard once; `--allow-blocked` overrides that for an agreed retry.

use std::time::Duration;

use fizz::device::{Dir, Keyboard};
use fizz::protocol::{self as p, Zone};

const TIMEOUT: Duration = Duration::from_millis(400);

fn main() {
    let api = match hidapi::HidApi::new() {
        Ok(api) => api,
        Err(e) => {
            eprintln!("Couldn't start HID access: {e}");
            std::process::exit(1);
        }
    };
    let mut kb = match Keyboard::open(&api) {
        Ok(kb) => kb,
        Err(e) => {
            eprintln!("Couldn't open the keyboard: {e}.");
            eprintln!("Check that it's plugged in. On Linux, install linux/99-fizz.rules.");
            std::process::exit(1);
        }
    };
    println!(
        "Opened {:04X}:{:04X} \"{}\" by \"{}\"",
        kb.ident.vendor_id, kb.ident.product_id, kb.ident.product_name, kb.ident.manufacturer
    );
    println!("  path {}", kb.ident.path);

    if std::env::args().any(|a| a == "--rewrite-keymap") {
        let ok = rewrite_keymap(&mut kb);
        std::process::exit(if ok { 0 } else { 2 });
    }
    if std::env::args().any(|a| a == "--rewrite-triggers") {
        let ok = rewrite_triggers(&mut kb);
        std::process::exit(if ok { 0 } else { 2 });
    }
    if std::env::args().any(|a| a == "--rewrite-socd") {
        let ok = rewrite_socd(&mut kb);
        std::process::exit(if ok { 0 } else { 2 });
    }
    if let Some(pos) = std::env::args().position(|a| a == "--dump-macro") {
        // Read-only: print a macro slot's raw bytes.
        kb.on_packet = None;
        let slot: usize = std::env::args().nth(pos + 1).and_then(|a| a.parse().ok()).unwrap_or(0);
        match kb.read_macro_raw(slot) {
            Ok(Some(raw)) => {
                for (i, row) in raw.chunks(32).enumerate() {
                    println!("  {:3}: {}", i * 32, p::hex(row));
                }
                println!("  parses as {:?}", p::parse_macro(&raw));
            }
            other => println!("  {other:?}"),
        }
        std::process::exit(0);
    }
    if std::env::args().any(|a| a == "--macro-write-all-test") {
        let ok = macro_write_all_test(&mut kb);
        std::process::exit(if ok { 0 } else { 2 });
    }
    if let Some(pos) = std::env::args().position(|a| a == "--delete-macro") {
        // The vendor app's delete command: 19 03 .. .. <index>.
        kb.on_packet = None;
        let slot: usize = std::env::args().nth(pos + 1).and_then(|a| a.parse().ok()).unwrap_or(usize::MAX);
        if slot >= p::MACRO_SLOTS {
            println!("usage: --delete-macro <slot 0-9>");
            std::process::exit(2);
        }
        let ok = kb.write_only(&p::macro_delete(slot)).is_ok();
        std::thread::sleep(Duration::from_millis(100));
        let after = kb.read_macro_raw(slot);
        match &after {
            Ok(Some(raw)) => println!(
                "delete sent {ok}; slot {slot} now starts {} (all FF: {})",
                p::hex(&raw[..16]),
                raw.iter().all(|b| *b == 0xFF)
            ),
            other => println!("delete sent {ok}; read back {other:?}"),
        }
        std::process::exit(0);
    }
    if let Some(pos) = std::env::args().position(|a| a == "--clear-macro") {
        // Write an empty (all-zero) slot, as the vendor app does for unused slots.
        kb.on_packet = None;
        let slot: usize = std::env::args().nth(pos + 1).and_then(|a| a.parse().ok()).unwrap_or(usize::MAX);
        if slot >= p::MACRO_SLOTS {
            println!("usage: --clear-macro <slot 0-9>");
            std::process::exit(2);
        }
        let empty = p::macro_bytes(slot, None);
        let ok = matches!(kb.write_macro_raw(slot, &empty), Ok(Some(back)) if back == empty);
        println!("slot {slot} cleared: {ok}");
        std::process::exit(if ok { 0 } else { 2 });
    }
    if std::env::args().any(|a| a == "--read-macros") {
        read_macros(&mut kb);
        std::process::exit(0);
    }
    if std::env::args().any(|a| a == "--macro-write-test") {
        let ok = macro_write_test(&mut kb);
        std::process::exit(if ok { 0 } else { 2 });
    }
    let arg_after = |flag: &str| {
        std::env::args()
            .position(|a| a == flag)
            .and_then(|i| std::env::args().nth(i + 1))
    };
    if let Some(file) = arg_after("--profile-export") {
        std::process::exit(if profile_export(&mut kb, &file) { 0 } else { 2 });
    }
    if let Some(file) = arg_after("--profile-restore") {
        std::process::exit(if profile_restore(&mut kb, &file) { 0 } else { 2 });
    }
    if std::env::args().any(|a| a == "--profile-roundtrip") {
        std::process::exit(if profile_roundtrip(&mut kb) { 0 } else { 2 });
    }
    if std::env::args().any(|a| a == "--survey") {
        survey(&mut kb);
        std::process::exit(0);
    }
    if std::env::args().any(|a| a == "--travel-stop") {
        // Safe cleanup if a travel test was interrupted before its stop.
        kb.on_packet = None;
        let stopped = kb.write_only(&p::travel_test_stop());
        let alive = kb.request_same("status", &p::status_request(), TIMEOUT).is_ok();
        println!("stop sent: {}, keyboard answers: {alive}", stopped.is_ok());
        std::process::exit(if stopped.is_ok() && alive { 0 } else { 2 });
    }
    if let Some(pos) = std::env::args().position(|a| a == "--travel-test") {
        let secs = std::env::args()
            .nth(pos + 1)
            .and_then(|a| a.parse().ok())
            .unwrap_or(3.0);
        let arg_after = |flag: &str| {
            std::env::args()
                .position(|a| a == flag)
                .and_then(|i| std::env::args().nth(i + 1))
        };
        let slots: Vec<usize> = match arg_after("--key").and_then(|a| a.parse().ok()) {
            Some(slot) => vec![slot],
            None => fizz::keymap::LAYOUT.iter().map(|k| k.slot).collect(),
        };
        let allow = std::env::args().any(|a| a == "--allow-blocked");
        let ok = travel_test(&mut kb, secs, &slots, allow);
        std::process::exit(if ok { 0 } else { 2 });
    }
    if std::env::args().any(|a| a == "--rewrite-colours") {
        let only = std::env::args()
            .position(|a| a == "--layer")
            .and_then(|i| std::env::args().nth(i + 1))
            .and_then(|a| a.parse::<u8>().ok());
        let ok = rewrite_colours(&mut kb, only);
        std::process::exit(if ok { 0 } else { 2 });
    }
    if let Some(pos) = std::env::args().position(|a| a == "--apply-light") {
        let args: Vec<String> = std::env::args().skip(pos + 1).take(7).collect();
        let ok = apply_light(&mut kb, &args);
        std::process::exit(if ok { 0 } else { 2 });
    }
    if std::env::args().any(|a| a == "--rewrite-poll-rate") {
        let ok = rewrite_poll_rate(&mut kb);
        std::process::exit(if ok { 0 } else { 2 });
    }
    kb.on_packet = Some(Box::new(|dir, packet| {
        let arrow = if dir == Dir::Out { ">>" } else { "<<" };
        println!("    {arrow} {}", p::hex(packet));
    }));

    println!("\nStatus");
    match kb.request_same("status", &p::status_request(), TIMEOUT) {
        Ok(r) => match p::parse_status(&r) {
            Some(s) => {
                println!("  mode byte: {}", s.mode);
                println!("  features: {}", s.support.features().join(", "));
                println!("  calibrated: {}", s.support.calibrated);
            }
            None => println!("  unexpected reply"),
        },
        Err(e) => println!("  {e}"),
    }

    println!("\nProduct info");
    match kb.request_pages(
        "product info",
        &p::product_info_request(),
        1,
        TIMEOUT,
        Duration::from_millis(300),
    ) {
        Ok(pages) => match p::parse_product_info(&pages) {
            Some(info) => {
                println!("  raw: {:?}", info.raw);
                println!("  product: {:?}", info.product());
                println!("  firmware: {:?}", info.firmware());
                println!("  built: {:?}", info.build_date);
            }
            None => println!("  unexpected reply"),
        },
        Err(e) => println!("  {e}"),
    }

    println!("\nSerial number");
    match kb.request_same("serial number", &p::serial_request(), TIMEOUT) {
        Ok(r) => println!("  {:?}", p::parse_serial(&r)),
        Err(e) => println!("  {e}"),
    }

    println!("\nPolling rate");
    match kb.request(
        "polling rate",
        &p::poll_rate_request(),
        |r| r[0] == p::cmd::PROFILE_TRIGGER && r[5] == 0x09,
        TIMEOUT,
    ) {
        Ok(r) => println!("  {:?} Hz", p::parse_poll_rate(&r)),
        Err(e) => println!("  {e}"),
    }

    println!("\nActuation config");
    let high_precision = kb
        .request_same("status", &p::status_request(), TIMEOUT)
        .ok()
        .and_then(|r| p::parse_status(&r))
        .is_some_and(|s| s.support.high_precision);
    let config = match kb.request(
        "trigger config",
        &p::trigger_config_request(),
        |r| r[0] == p::cmd::PROFILE_TRIGGER && r[5] == 0x04,
        TIMEOUT,
    ) {
        Ok(r) => {
            let c = p::parse_trigger_config(&r, high_precision);
            println!("  {c:?}");
            if let Some(c) = c {
                println!(
                    "  actuation range {:.2}–{:.2} mm in {:.2} mm steps",
                    c.mm(c.min_steps),
                    c.mm(c.max_steps),
                    c.unit_mm
                );
            }
            c
        }
        Err(e) => {
            println!("  {e}");
            None
        }
    };

    println!("\nPer-key actuation (quiet: first and last few keys printed raw)");
    let quiet = kb.on_packet.take();
    let mut summary = std::collections::BTreeMap::<p::KeyTrigger, Vec<&str>>::new();
    let mut raw_shown = 0;
    for key in fizz::keymap::LAYOUT {
        match kb.request(
            "key trigger",
            &p::key_trigger_request(key.slot),
            |r| r[0] == p::cmd::PROFILE_TRIGGER && r[5] == 0x05,
            TIMEOUT,
        ) {
            Ok(r) => {
                if raw_shown < 3 || key.slot == 120 {
                    println!("    {:>6} << {}", key.label, p::hex(&r[..20]));
                    raw_shown += 1;
                }
                if let Some(t) = p::parse_key_trigger(&r) {
                    summary.entry(t).or_default().push(key.label);
                }
            }
            Err(e) => println!("  {}: {e}", key.label),
        }
    }
    kb.on_packet = quiet;
    for (t, keys) in &summary {
        let mm = |s: u16| config.map(|c| format!("{:.2} mm", c.mm(s))).unwrap_or_default();
        println!(
            "  mode {:#04x} travel {} ({}) press {} ({}) release {} ({}): {} keys {:?}",
            t.mode,
            t.travel,
            mm(t.travel),
            t.press,
            mm(t.press),
            t.release,
            mm(t.release),
            keys.len(),
            if keys.len() > 8 { &keys[..8] } else { &keys[..] }
        );
    }

    println!("\nPer-key colours");
    match kb.request(
        "colour layer",
        &p::colour_layer_request(),
        |r| r[0] == p::cmd::CUSTOM_COLOURS && r[1] != 0x80 && r[1] != 0x81,
        TIMEOUT,
    ) {
        Ok(r) => println!("  active layer: {:?}", p::parse_colour_layer(&r)),
        Err(e) => println!("  active layer: {e}"),
    }
    let quiet = kb.on_packet.take();
    for layer in 0..2u8 {
        match kb.request_pages(
            "colours",
            &p::colours_request(layer),
            p::COLOUR_LAST_PAGE,
            TIMEOUT,
            Duration::from_millis(300),
        ) {
            Ok(pages) => match p::parse_colours(layer, &pages) {
                Some(colours) => {
                    let mut counts = std::collections::BTreeMap::<[u8; 3], usize>::new();
                    for key in fizz::keymap::LAYOUT {
                        *counts.entry(colours[key.slot]).or_default() += 1;
                    }
                    println!(
                        "  layer {layer}: {} pages, colours on the 61 keys: {counts:02X?}",
                        pages.len()
                    );
                }
                None => println!("  layer {layer}: incomplete, got {} pages", pages.len()),
            },
            Err(e) => println!("  layer {layer}: {e}"),
        }
    }
    kb.on_packet = quiet;

    for layer in [p::Layer::Default, p::Layer::Fn] {
        println!("\nKey map: {layer:?}");
        match kb.request_pages(
            "key map",
            &p::keymap_request(layer),
            p::KEYMAP_LAST_PAGE,
            TIMEOUT,
            Duration::from_millis(300),
        ) {
            Ok(pages) => match p::parse_keymap(layer, &pages) {
                Some(keys) => {
                    for (slot, k) in keys.iter().enumerate() {
                        if *k != p::KeyAction::default() {
                            println!(
                                "  slot {slot:3} (row {}, col {:2}): {:02X} {:02X} {:02X} {:02X}",
                                slot / 22,
                                slot % 22,
                                k.kind,
                                k.code,
                                k.arg1,
                                k.arg2
                            );
                        }
                    }
                }
                None => println!("  incomplete: got {} pages", pages.len()),
            },
            Err(e) => println!("  {e}"),
        }
    }

    for zone in [Zone::Keys, Zone::Side] {
        println!("\nLighting: {zone:?}");
        match kb.request_same("light modes", &p::light_modes_request(zone), TIMEOUT) {
            Ok(r) => match p::parse_mode_list(zone, &r) {
                Some(list) => {
                    let names: Vec<_> = list.modes.iter().map(|m| p::mode_name(*m)).collect();
                    println!("  modes: {names:?}");
                    println!(
                        "  max brightness {}, max speed {}",
                        list.max_brightness, list.max_speed
                    );
                }
                None => println!("  unexpected reply"),
            },
            Err(e) => println!("  {e}"),
        }
        match kb.request_same("light config", &p::light_config_request(zone), TIMEOUT) {
            Ok(r) => println!("  config: {:?}", p::parse_light_config(zone, &r)),
            Err(e) => println!("  {e}"),
        }
    }
}

fn layer_hex(keys: &[p::KeyAction]) -> String {
    keys.chunks(11)
        .map(|row| {
            row.iter()
                .map(|k| format!("{:02X}{:02X}{:02X}{:02X}", k.kind, k.code, k.arg1, k.arg2))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Write both layers back unchanged and verify. Returns true on success.
fn rewrite_keymap(kb: &mut Keyboard) -> bool {
    use p::Layer;
    let layers = [Layer::Default, Layer::Fn];

    println!("\nReading both layers");
    let mut before = Vec::new();
    for layer in layers {
        match kb.read_keymap(layer) {
            Ok(Some(keys)) => before.push(keys),
            Ok(None) => {
                println!("  {layer:?}: incomplete read, stopping without writing anything");
                return false;
            }
            Err(e) => {
                println!("  {layer:?}: {e}, stopping without writing anything");
                return false;
            }
        }
    }

    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let path = format!("backups/keymap-{stamp}.txt");
    let text = format!(
        "# Fizz key map backup, 132 slots per layer, [kind code arg1 arg2] per slot\n[default]\n{}\n[fn]\n{}\n",
        layer_hex(&before[0]),
        layer_hex(&before[1])
    );
    if let Err(e) = std::fs::create_dir_all("backups").and_then(|_| std::fs::write(&path, text)) {
        println!("  couldn't write the backup ({e}), stopping without writing anything");
        return false;
    }
    println!("  backup saved to {path}");

    let mut ok = true;
    for (layer, keys) in layers.iter().zip(&before) {
        println!("\nWriting {layer:?} layer back unchanged");
        match kb.write_keymap(*layer, keys) {
            Ok(r) => {
                println!(
                    "  pages sent {}, acknowledged {}, storing flag seen {}, read-back matches {}",
                    r.pages, r.acked, r.storing_seen, r.verified
                );
                ok &= r.verified;
            }
            Err(e) => {
                println!("  write failed: {e}");
                ok = false;
            }
        }
    }

    println!("\nReading both layers again");
    for (layer, keys) in layers.iter().zip(&before) {
        match kb.read_keymap(*layer) {
            Ok(Some(after)) if after == *keys => println!("  {layer:?}: identical to before"),
            Ok(Some(after)) => {
                ok = false;
                let diffs: Vec<_> = (0..after.len()).filter(|i| after[*i] != keys[*i]).collect();
                println!("  {layer:?}: DIFFERENT at slots {diffs:?}");
            }
            Ok(None) => {
                ok = false;
                println!("  {layer:?}: incomplete read");
            }
            Err(e) => {
                ok = false;
                println!("  {layer:?}: {e}");
            }
        }
    }
    println!("\n{}", if ok { "PASS" } else { "FAIL" });
    ok
}

/// Write every key's actuation settings back unchanged and verify.
fn rewrite_triggers(kb: &mut Keyboard) -> bool {
    let slots: Vec<usize> = fizz::keymap::LAYOUT.iter().map(|k| k.slot).collect();
    println!("\nReading actuation for {} keys", slots.len());
    let before = match kb.read_key_triggers(&slots) {
        Ok(v) => v,
        Err(e) => {
            println!("  {e}, stopping without writing anything");
            return false;
        }
    };
    if before.iter().any(Option::is_none) {
        println!("  some keys didn't answer, stopping without writing anything");
        return false;
    }
    let before: Vec<p::KeyTrigger> = before.into_iter().flatten().collect();

    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let path = format!("backups/triggers-{stamp}.txt");
    let mut text = String::from("# Fizz actuation backup: slot mode travel press release (steps)\n");
    for (slot, t) in slots.iter().zip(&before) {
        text.push_str(&format!("{slot} {:#04x} {} {} {}\n", t.mode, t.travel, t.press, t.release));
    }
    if let Err(e) = std::fs::create_dir_all("backups").and_then(|_| std::fs::write(&path, text)) {
        println!("  couldn't write the backup ({e}), stopping without writing anything");
        return false;
    }
    println!("  backup saved to {path}");

    let mut groups: Vec<(p::KeyTrigger, Vec<usize>)> = Vec::new();
    for (slot, t) in slots.iter().zip(&before) {
        match groups.iter_mut().find(|(g, _)| g == t) {
            Some((_, s)) => s.push(*slot),
            None => groups.push((*t, vec![*slot])),
        }
    }
    println!("\nWriting {} setting group(s) back unchanged", groups.len());
    let ok = match kb.write_key_triggers(&groups) {
        Ok(r) => {
            let missing = r.stored.iter().filter(|(_, t)| t.is_none()).count();
            println!("  read-back matches {}, keys not answering {missing}", r.verified);
            r.verified
        }
        Err(e) => {
            println!("  write failed: {e}");
            false
        }
    };
    println!("\n{}", if ok { "PASS" } else { "FAIL" });
    ok
}

/// Write the current polling rate back unchanged and verify.
fn rewrite_poll_rate(kb: &mut Keyboard) -> bool {
    let read = |kb: &mut Keyboard| {
        kb.request(
            "polling rate",
            &p::poll_rate_request(),
            |r| r[0] == p::cmd::PROFILE_TRIGGER && r[5] == 0x09,
            TIMEOUT,
        )
        .ok()
        .and_then(|r| p::parse_poll_rate(&r))
    };
    let Some(before) = read(kb) else {
        println!("\nCouldn't read the polling rate, stopping without writing anything");
        return false;
    };
    println!("\nPolling rate is {before} Hz, writing it back unchanged");
    let Some(packet) = p::poll_rate_write(before) else {
        return false;
    };
    if let Err(e) = kb.write_only(&packet) {
        println!("  write failed: {e}");
        return false;
    }
    std::thread::sleep(Duration::from_millis(100));
    let after = read(kb);
    println!("  read back {after:?} Hz");
    let ok = after == Some(before);
    println!("\n{}", if ok { "PASS" } else { "FAIL" });
    ok
}

/// Run the live travel test for `secs` seconds and summarise the stream.
fn travel_test(kb: &mut Keyboard, secs: f32, slots: &[usize], allow_blocked: bool) -> bool {
    kb.on_packet = None;
    println!(
        "\nStarting the travel test on {} key(s) {:?} for {secs} s",
        slots.len(),
        &slots[..slots.len().min(4)]
    );
    let start = p::travel_test_start(slots);
    let sent = if allow_blocked {
        println!("  --allow-blocked: sending the blocked start command on purpose");
        kb.send_overriding_block(&start)
    } else {
        kb.write_only(&start)
    };
    if let Err(e) = sent {
        println!("  start failed: {e}");
        return false;
    }
    let until = std::time::Instant::now() + Duration::from_secs_f32(secs);
    let mut packets = 0usize;
    let mut other = 0usize;
    let mut shown = 0;
    let mut per_key = std::collections::BTreeMap::<usize, (usize, u16, u16)>::new();
    while std::time::Instant::now() < until {
        match kb.recv(Duration::from_millis(50)) {
            Ok(Some(packet)) => match p::parse_travel(&packet) {
                Some((slot, travel)) => {
                    packets += 1;
                    if shown < 5 {
                        println!("    << {}", p::hex(&packet[..12]));
                        shown += 1;
                    }
                    let e = per_key.entry(slot).or_insert((0, u16::MAX, 0));
                    e.0 += 1;
                    e.1 = e.1.min(travel);
                    e.2 = e.2.max(travel);
                }
                None => {
                    other += 1;
                    if other <= 3 {
                        println!("    other << {}", p::hex(&packet[..12]));
                    }
                }
            },
            Ok(None) => {}
            Err(e) => {
                println!("  read failed: {e}");
                break;
            }
        }
    }
    // Always stop, whatever happened above.
    let stopped = kb.write_only(&p::travel_test_stop());
    println!("  stop sent: {}", if stopped.is_ok() { "yes" } else { "FAILED" });
    println!("  travel packets {packets}, other packets {other}, keys seen {}", per_key.len());
    for (slot, (n, lo, hi)) in &per_key {
        let label = fizz::keymap::LAYOUT
            .iter()
            .find(|k| k.slot == *slot)
            .map_or("?", |k| k.label);
        println!("    {label:>7} (slot {slot:3}): {n} reports, travel {lo}..{hi}");
    }
    // Confirm the keyboard still answers normal requests after stopping.
    let alive = kb.request_same("status", &p::status_request(), TIMEOUT).is_ok();
    println!("  keyboard answers after stop: {alive}");
    stopped.is_ok() && alive
}

/// Write both per-key colour layers back unchanged and verify.
fn rewrite_colours(kb: &mut Keyboard, only: Option<u8>) -> bool {
    println!("\nReading both colour layers");
    let mut before = Vec::new();
    for layer in 0..2u8 {
        match kb.read_colours(layer) {
            Ok(Some(c)) => before.push(c),
            Ok(None) => {
                println!("  layer {layer}: incomplete read, stopping without writing anything");
                return false;
            }
            Err(e) => {
                println!("  layer {layer}: {e}, stopping without writing anything");
                return false;
            }
        }
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let path = format!("backups/colours-{stamp}.txt");
    let mut text = String::from("# Fizz per-key colour backup: 132 slots per layer, RRGGBB\n");
    for (layer, colours) in before.iter().enumerate() {
        text.push_str(&format!("[layer {layer}]\n"));
        for row in colours.chunks(22) {
            let line: Vec<String> = row
                .iter()
                .map(|c| format!("{:02X}{:02X}{:02X}", c[0], c[1], c[2]))
                .collect();
            text.push_str(&line.join(" "));
            text.push('\n');
        }
    }
    if let Err(e) = std::fs::create_dir_all("backups").and_then(|_| std::fs::write(&path, text)) {
        println!("  couldn't write the backup ({e}), stopping without writing anything");
        return false;
    }
    println!("  backup saved to {path}");

    let mut ok = true;
    for (layer, colours) in before.iter().enumerate() {
        if only.is_some_and(|o| o as usize != layer) {
            continue;
        }
        println!("\nWriting colour layer {layer} back unchanged");
        match kb.write_colours(layer as u8, colours) {
            Ok(r) => {
                println!(
                    "  pages sent {}, acknowledged {}, read-back matches {}",
                    r.pages, r.acked, r.verified
                );
                ok &= r.verified;
            }
            Err(e) => {
                println!("  write failed: {e}");
                ok = false;
            }
        }
    }
    let layer_now = kb.read_colour_layer().ok().flatten();
    let light_now = kb
        .request_same("light config", &p::light_config_request(p::Zone::Keys), TIMEOUT)
        .ok()
        .and_then(|r| p::parse_light_config(p::Zone::Keys, &r));
    println!("\nAfter the write: active colour layer {layer_now:?}, lighting {light_now:?}");
    println!("\n{}", if ok { "PASS" } else { "FAIL" });
    ok
}

/// Set the key lighting: mode, brightness, speed, foreground RRGGBB,
/// background RRGGBB, direction, rainbow (0/1). Keeps power and timer as read.
fn apply_light(kb: &mut Keyboard, args: &[String]) -> bool {
    let rgb = |s: &str| -> Option<[u8; 3]> {
        let v = u32::from_str_radix(s, 16).ok()?;
        Some([(v >> 16) as u8, (v >> 8) as u8, v as u8])
    };
    let parsed = (|| {
        Some((
            args.first()?.parse::<u8>().ok()?,
            args.get(1)?.parse::<u8>().ok()?,
            args.get(2)?.parse::<u8>().ok()?,
            rgb(args.get(3)?)?,
            rgb(args.get(4)?)?,
            args.get(5)?.parse::<u8>().ok()?,
            args.get(6)?.parse::<u8>().ok()? != 0,
        ))
    })();
    let Some((mode, brightness, speed, foreground, background, direction, full_color)) = parsed else {
        println!("usage: --apply-light <mode> <brightness> <speed> <fg RRGGBB> <bg RRGGBB> <direction> <rainbow 0|1>");
        return false;
    };
    let read = |kb: &mut Keyboard| {
        kb.request_same("light config", &p::light_config_request(p::Zone::Keys), TIMEOUT)
            .ok()
            .and_then(|r| p::parse_light_config(p::Zone::Keys, &r))
    };
    let Some(before) = read(kb) else {
        println!("\nCouldn't read the lighting, stopping without writing anything");
        return false;
    };
    println!("\nLighting before: {before:?}");
    let want = p::LightConfig {
        mode,
        brightness,
        speed,
        foreground,
        background,
        direction,
        full_color,
        power: before.power,
        timer: before.timer,
    };
    if let Err(e) = kb.write_only(&p::light_config_write(p::Zone::Keys, &want)) {
        println!("  write failed: {e}");
        return false;
    }
    std::thread::sleep(Duration::from_millis(100));
    let after = read(kb);
    println!("Lighting after:  {after:?}");
    let ok = after == Some(want);
    println!("\n{}", if ok { "PASS" } else { "FAIL" });
    ok
}

/// Read-only survey: send the vendor app's read command for each feature we
/// haven't built yet and show what comes back.
fn survey(kb: &mut Keyboard) {
    kb.on_packet = None;
    let read = |cmd: u8, sub: u8, b4: u8, b5: u8| {
        let mut pkt = [0u8; p::PACKET_LEN];
        pkt[0] = cmd;
        pkt[1] = sub;
        pkt[4] = b4;
        pkt[5] = b5;
        pkt
    };
    // (name, packet, last page to wait for)
    let probes: Vec<(&str, p::Packet, u16)> = vec![
        ("SOCD entries (0x24 01)", read(0x24, 0x01, 0, 0), 1),
        ("SOCD power (0x24 02)", read(0x24, 0x02, 0, 0), 0),
        ("Advanced keys (0x22 80)", read(0x22, 0x80, 0, 0), 0x27),
        ("Dead zones (0x26 00)", read(0x26, 0x00, 0, 0), 4),
        ("Switch types list (0x25 00)", read(0x25, 0x00, 0, 0), 0),
        ("Switch type per key (0x25 02)", read(0x25, 0x02, 0, 0), 2),
        ("Macro slot 0 (0x19 80)", read(0x19, 0x80, 0, 0), 4),
        ("Fn-editable keys (0x0E)", read(0x0E, 0x00, 0, 0), 0),
    ];
    for (name, pkt, last) in probes {
        println!("\n{name}");
        match kb.request_pages(name_static(name), &pkt, last, TIMEOUT, Duration::from_millis(300)) {
            Ok(pages) => {
                println!("  {} page(s)", pages.len());
                for r in pages.iter().take(3) {
                    println!("    << {}", p::hex(&r[..28]));
                }
                if pages.len() > 3 {
                    println!("    … last << {}", p::hex(&pages[pages.len() - 1][..28]));
                }
            }
            Err(e) => println!("  {e}"),
        }
    }
    println!("\nProfile index (0x21 .. 11)");
    let pkt = read(0x21, 0x00, 0x00, 0x11);
    match kb.request(
        "profile index",
        &pkt,
        |r| r[0] == 0x21 && r[5] == 0x11,
        Duration::from_millis(1000),
    ) {
        Ok(r) => println!("    << {}", p::hex(&r[..16])),
        Err(e) => println!("  {e}"),
    }
    println!("\nStatus lock and mode bytes");
    if let Ok(r) = kb.request_same("status", &p::status_request(), TIMEOUT) {
        let sup = &r[5..0x17];
        println!(
            "  lock keys supported {}, lock byte {:#04x}, SOCD flag {}, 6-key flag byte {:#04x}, unknown flags byte {:#04x}",
            sup[0] == 1,
            sup[15],
            sup[9] == 1,
            sup[17],
            sup[15]
        );
    }
}

fn name_static(_: &str) -> &'static str {
    "survey read"
}

/// Write the SOCD pairs and on/off state back unchanged and verify.
fn rewrite_socd(kb: &mut Keyboard) -> bool {
    kb.on_packet = None;
    let (pairs, power) = match (kb.read_socd_pairs(), kb.read_socd_power()) {
        (Ok(Some(pairs)), Ok(Some(power))) => (pairs, power),
        _ => {
            println!("\nCouldn't read SOCD, stopping without writing anything");
            return false;
        }
    };
    let used: Vec<_> = pairs.iter().flatten().collect();
    println!("\nSOCD is {}, {} pair(s): {used:?}", if power.on { "on" } else { "off" }, used.len());
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let path = format!("backups/socd-{stamp}.txt");
    let mut text = format!(
        "# Fizz SOCD backup: power {} (codes {:02X} {:02X}); slot state mode key1 key2\n",
        power.on as u8, power.code1, power.code2
    );
    for (i, p) in pairs.iter().enumerate() {
        if let Some(p) = p {
            text.push_str(&format!("{i} {:02X} {:02X} {:02X} {:02X}\n", p.state, p.mode, p.key1, p.key2));
        }
    }
    if let Err(e) = std::fs::create_dir_all("backups").and_then(|_| std::fs::write(&path, text)) {
        println!("  couldn't write the backup ({e}), stopping without writing anything");
        return false;
    }
    println!("  backup saved to {path}");

    println!("\nWriting the pairs back unchanged");
    let pairs_ok = match kb.write_socd_pairs(&pairs) {
        Ok(Some(after)) => {
            println!("  read-back matches {}", after == pairs);
            after == pairs
        }
        Ok(None) => {
            println!("  read-back incomplete");
            false
        }
        Err(e) => {
            println!("  write failed: {e}");
            false
        }
    };
    let power_after = kb.read_socd_power().ok().flatten();
    println!("  SOCD on/off after writing pairs: {power_after:?}");
    let power_kept = power_after == Some(power);

    println!("\nWriting on/off back unchanged ({})", if power.on { "on" } else { "off" });
    let power_ok = match kb.set_socd_power(power.on) {
        Ok(after) => {
            println!("  read back {after:?}");
            after == Some(power)
        }
        Err(e) => {
            println!("  write failed: {e}");
            false
        }
    };
    let ok = pairs_ok && power_kept && power_ok;
    println!("\n{}", if ok { "PASS" } else { "FAIL" });
    ok
}

/// Read-only: list what's in each macro slot and which keys use macros.
fn read_macros(kb: &mut Keyboard) {
    kb.on_packet = None;
    println!("\nMacro slots");
    for slot in 0..p::MACRO_SLOTS {
        match kb.read_macro_raw(slot) {
            Ok(Some(raw)) => match p::parse_macro(&raw) {
                Some(m) => println!(
                    "  slot {slot}: {} ({}x), {} steps: {:?}",
                    p::macro_play::name(m.play),
                    m.count,
                    m.steps.len(),
                    &m.steps[..m.steps.len().min(4)]
                ),
                None => println!("  slot {slot}: empty (header {})", p::hex(&raw[..8])),
            },
            Ok(None) => println!("  slot {slot}: no page 0"),
            Err(e) => println!("  slot {slot}: {e}"),
        }
    }
    for layer in [p::Layer::Default, p::Layer::Fn] {
        if let Ok(Some(keys)) = kb.read_keymap(layer) {
            let bound: Vec<_> = keys
                .iter()
                .enumerate()
                .filter(|(_, k)| k.kind == fizz::keymap::kind::MACRO)
                .map(|(slot, k)| (slot, k.arg1))
                .collect();
            println!("  {layer:?} layer keys bound to macros (key slot, macro slot): {bound:?}");
        }
    }
}

/// Write a small test macro into an empty slot that no key uses, verify it,
/// then put the slot's original bytes back and verify that too.
fn macro_write_test(kb: &mut Keyboard) -> bool {
    kb.on_packet = None;
    let mut bound = Vec::new();
    for layer in [p::Layer::Default, p::Layer::Fn] {
        match kb.read_keymap(layer) {
            Ok(Some(keys)) => bound.extend(
                keys.iter()
                    .filter(|k| k.kind == fizz::keymap::kind::MACRO)
                    .map(|k| k.arg1 as usize),
            ),
            _ => {
                println!("\nCouldn't read the key map, stopping without writing anything");
                return false;
            }
        }
    }
    let mut chosen = None;
    for slot in (0..p::MACRO_SLOTS).rev() {
        if bound.contains(&slot) {
            continue;
        }
        if let Ok(Some(raw)) = kb.read_macro_raw(slot)
            && p::parse_macro(&raw).is_none()
        {
            chosen = Some((slot, raw));
            break;
        }
    }
    let Some((slot, original)) = chosen else {
        println!("\nNo empty, unused macro slot; stopping without writing anything");
        return false;
    };
    println!("\nUsing empty slot {slot} (no key uses it); original header {}", p::hex(&original[..8]));

    let test = p::Macro {
        play: p::macro_play::TIMES,
        count: 2,
        steps: vec![
            p::MacroStep { press: true, mouse: false, code: 0x04, delay_ms: 30 },
            p::MacroStep { press: false, mouse: false, code: 0x04, delay_ms: 30 },
        ],
    };
    let raw = p::macro_bytes(slot, Some(&test));
    let wrote = match kb.write_macro_raw(slot, &raw) {
        Ok(Some(back)) => {
            let same = back == raw;
            println!("  test macro written, read-back matches {same}, parses as {:?}", p::parse_macro(&back));
            same
        }
        Ok(None) => {
            println!("  test macro read-back incomplete");
            false
        }
        Err(e) => {
            println!("  write failed: {e}");
            false
        }
    };

    // Erased flash (all FF) can't be written back: the keyboard ignores a
    // page with an FF header. Clear it the vendor app's way, with zeros.
    let restore = if original.iter().all(|b| *b == 0xFF) {
        p::macro_bytes(slot, None)
    } else {
        original
    };
    println!("\nClearing the slot again (empty, as the vendor app writes it)");
    let restored = match kb.write_macro_raw(slot, &restore) {
        Ok(Some(back)) => {
            let same = back == restore && p::parse_macro(&back).is_none();
            println!("  read-back is empty {same}");
            same
        }
        Ok(None) => {
            println!("  read-back incomplete");
            false
        }
        Err(e) => {
            println!("  restore failed: {e}");
            false
        }
    };
    let light = kb
        .request_same("light config", &p::light_config_request(p::Zone::Keys), TIMEOUT)
        .ok()
        .and_then(|r| p::parse_light_config(p::Zone::Keys, &r));
    println!("  lighting afterwards: {light:?}");
    let ok = wrote && restored;
    println!("\n{}", if ok { "PASS" } else { "FAIL" });
    ok
}

/// Write all 10 slots in order, like the vendor app (test macro in slot 9,
/// the rest empty), check, then write all 10 empty and check again. Only
/// runs when every slot is empty and no key uses a macro.
fn macro_write_all_test(kb: &mut Keyboard) -> bool {
    kb.on_packet = None;
    for layer in [p::Layer::Default, p::Layer::Fn] {
        match kb.read_keymap(layer) {
            Ok(Some(keys)) if keys.iter().all(|k| k.kind != fizz::keymap::kind::MACRO) => {}
            _ => {
                println!("\nA key uses a macro (or the key map didn't read); stopping");
                return false;
            }
        }
    }
    for slot in 0..p::MACRO_SLOTS {
        match kb.read_macro_raw(slot) {
            Ok(Some(raw)) if p::parse_macro(&raw).is_none() => {}
            _ => {
                println!("\nSlot {slot} isn't empty (or didn't read); stopping");
                return false;
            }
        }
    }
    let test = p::Macro {
        play: p::macro_play::TIMES,
        count: 2,
        steps: vec![
            p::MacroStep { press: true, mouse: false, code: 0x04, delay_ms: 30 },
            p::MacroStep { press: false, mouse: false, code: 0x04, delay_ms: 30 },
        ],
    };
    let write_all = |kb: &mut Keyboard, with_test: bool| -> Option<Vec<[u8; p::MACRO_BYTES]>> {
        let wanted: Vec<_> = (0..p::MACRO_SLOTS)
            .map(|slot| p::macro_bytes(slot, (with_test && slot == 9).then_some(&test)))
            .collect();
        for (slot, raw) in wanted.iter().enumerate() {
            if let Err(e) = kb.write_macro_raw(slot, raw) {
                println!("  slot {slot} write failed: {e}");
                return None;
            }
        }
        // Read everything again after the last write.
        let mut back = Vec::new();
        for slot in 0..p::MACRO_SLOTS {
            back.push(kb.read_macro_raw(slot).ok().flatten()?);
        }
        let ok = back == wanted;
        println!("  all 10 slots read back as written: {ok}");
        if !ok {
            for (slot, (b, w)) in back.iter().zip(&wanted).enumerate() {
                if b != w {
                    println!("    slot {slot}: got {} want {}", p::hex(&b[..12]), p::hex(&w[..12]));
                }
            }
        }
        ok.then_some(back)
    };
    println!("\nWriting all 10 slots: test macro in slot 9, the rest empty");
    let first = write_all(kb, true).is_some();
    println!("\nWriting all 10 slots empty again");
    let second = write_all(kb, false).is_some();
    let ok = first && second;
    println!("\n{}", if ok { "PASS" } else { "FAIL" });
    ok
}

fn print_sections(profile: &fizz::profile::Profile) {
    let names: Vec<_> = profile.sections().iter().map(|s| s.name()).collect();
    println!(
        "  product {:?}, firmware {:?}; sections: {}",
        profile.product,
        profile.firmware,
        names.join(", ")
    );
}

fn profile_export(kb: &mut Keyboard, file: &str) -> bool {
    kb.on_packet = None;
    match fizz::profile::read(kb) {
        Ok(profile) => {
            if let Err(e) = std::fs::write(file, profile.to_json()) {
                println!("\ncouldn't write {file}: {e}");
                return false;
            }
            println!("\nsaved {file}");
            print_sections(&profile);
            true
        }
        Err(e) => {
            println!("\ncouldn't read the keyboard: {e}");
            false
        }
    }
}

/// Which sections of two profiles differ (ignoring the save time).
fn differing(a: &fizz::profile::Profile, b: &fizz::profile::Profile) -> Vec<&'static str> {
    use fizz::profile::Section;
    let mut out = Vec::new();
    let same = [
        (Section::Macros, a.macros == b.macros),
        (Section::KeyMapDefault, a.keymap_default == b.keymap_default),
        (Section::KeyMapFn, a.keymap_fn == b.keymap_fn),
        (Section::Colours, a.colours == b.colours && a.colour_layer == b.colour_layer),
        (Section::Actuation, a.triggers == b.triggers),
        (Section::SocdPairs, a.socd_pairs == b.socd_pairs),
        (Section::SocdPower, a.socd_on == b.socd_on),
        (Section::PollRate, a.poll_rate_hz == b.poll_rate_hz),
        (Section::Lighting, a.light == b.light),
    ];
    for (section, same) in same {
        if !same {
            out.push(section.name());
        }
    }
    out
}

fn apply_and_report(kb: &mut Keyboard, profile: &fizz::profile::Profile) -> bool {
    use fizz::profile::Outcome;
    match fizz::profile::apply(kb, profile) {
        Err(problems) => {
            println!("  the profile isn't safe to write:");
            for p in problems {
                println!("    - {p}");
            }
            false
        }
        Ok(results) => {
            let mut ok = true;
            for r in &results {
                let line = match &r.outcome {
                    Outcome::Applied => "applied and read back identical".to_owned(),
                    Outcome::NotInProfile => "not in the profile".to_owned(),
                    Outcome::Mismatch(m) => {
                        ok = false;
                        format!("MISMATCH: {m}")
                    }
                    Outcome::Failed(m) => {
                        ok = false;
                        format!("FAILED: {m}")
                    }
                };
                println!("  {:<20} {line}", r.section.name());
            }
            ok
        }
    }
}

fn backup_profile(kb: &mut Keyboard, name: &str) -> Option<fizz::profile::Profile> {
    let current = fizz::profile::read(kb).ok()?;
    let file = format!("backups/{name}-{}.json", fizz::profile::file_stamp(current.saved_at));
    std::fs::create_dir_all("backups").ok()?;
    std::fs::write(&file, current.to_json()).ok()?;
    println!("  backup of the current settings: {file}");
    Some(current)
}

fn profile_restore(kb: &mut Keyboard, file: &str) -> bool {
    kb.on_packet = None;
    let text = match std::fs::read_to_string(file) {
        Ok(t) => t,
        Err(e) => {
            println!("\ncouldn't read {file}: {e}");
            return false;
        }
    };
    let profile = match fizz::profile::Profile::from_json(&text) {
        Ok(p) => p,
        Err(e) => {
            println!("\n{e}");
            return false;
        }
    };
    println!("\nRestoring {file}");
    print_sections(&profile);
    if backup_profile(kb, "before-restore").is_none() {
        println!("  couldn't back up the current settings, stopping without writing anything");
        return false;
    }
    let ok = apply_and_report(kb, &profile);
    println!("\n{}", if ok { "PASS" } else { "FAIL" });
    ok
}

/// Read everything, write it all back, read again, and compare. Ends with
/// the keyboard in the state it started in, or says what differs.
fn profile_roundtrip(kb: &mut Keyboard) -> bool {
    kb.on_packet = None;
    println!("\nReading every setting");
    let Some(before) = backup_profile(kb, "roundtrip") else {
        println!("  couldn't read or save the settings, stopping without writing anything");
        return false;
    };
    print_sections(&before);
    println!("\nWriting everything back");
    let applied = apply_and_report(kb, &before);
    println!("\nReading everything again");
    let Ok(after) = fizz::profile::read(kb) else {
        println!("  couldn't read the keyboard");
        return false;
    };
    let diff = differing(&before, &after);
    if diff.is_empty() {
        println!("  identical to before");
    } else {
        println!("  DIFFERENT: {}", diff.join(", "));
    }
    let ok = applied && diff.is_empty();
    println!("\n{}", if ok { "PASS" } else { "FAIL" });
    ok
}
