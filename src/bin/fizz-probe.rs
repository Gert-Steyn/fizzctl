//! Connection test. By default it sends only query commands and prints what
//! the keyboard reports, with every packet in hex.
//!
//! ```text
//! cargo run --bin fizz-probe
//! cargo run --bin fizz-probe -- --rewrite-keymap
//! cargo run --bin fizz-probe -- --rewrite-triggers
//! ```
//!
//! `--rewrite-keymap` tests the key-map save path without changing anything:
//! it backs up both layers to `backups/`, writes each one back unchanged, and
//! checks that the keyboard still holds exactly the same bytes.
//! `--rewrite-triggers` does the same for every key's actuation settings.

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
