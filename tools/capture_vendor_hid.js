// Paste into the browser console (F12 -> Console) on the vendor's configurator page, after the
// keyboard is connected there. It only OBSERVES: it records the HID reports the page sends to the
// keyboard and the ones the keyboard sends back, and never sends anything itself.
//
//   1. Paste this whole file into the console and press Enter.
//   2. Do the thing to capture in the page (e.g. start the live key test, press a few keys, stop it).
//   3. Run:  copy(__fizz.dump())     and paste the result somewhere to read it.
//   4. __fizz.stop() removes the recorder.
(() => {
  if (window.__fizz) window.__fizz.stop();

  const t0 = performance.now();
  const stamp = () => Math.round(performance.now() - t0);
  const hex = (b) => Array.from(b, (x) => x.toString(16).padStart(2, '0')).join(' ');
  const trim = (b) => {
    let n = b.length;
    while (n > 5 && b[n - 1] === 0) n--;
    return hex(b.slice(0, n)) + (n < b.length ? ' ..' : '');
  };

  const outgoing = []; // runs of identical packets: {first, last, n, hex, id}
  const incoming = {}; // "cmd/sub" -> {n, first, examples: [..]}
  const travel = { n: 0, first: [], perSecond: {}, keys: new Set(), max: 0 };
  const originals = [];
  const listeners = [];

  const bytesOf = (data) => {
    if (data instanceof ArrayBuffer) return new Uint8Array(data.slice(0));
    return new Uint8Array(data.buffer.slice(data.byteOffset, data.byteOffset + data.byteLength));
  };

  const recordOut = (id, data) => {
    const b = bytesOf(data);
    const h = trim(b);
    const last = outgoing[outgoing.length - 1];
    if (last && last.hex === h && last.id === id) {
      last.n++;
      last.last = stamp();
    } else {
      outgoing.push({ first: stamp(), last: stamp(), n: 1, hex: h, id });
    }
  };

  const recordIn = (id, data) => {
    const b = bytesOf(data);
    const t = stamp();
    // 0x21 sub-command 1 is the live key-travel stream.
    if (b[0] === 0x21 && b[5] === 0x01) {
      travel.n++;
      if (travel.first.length < 30) travel.first.push(t + 'ms  ' + trim(b));
      const sec = Math.floor(t / 1000);
      travel.perSecond[sec] = (travel.perSecond[sec] || 0) + 1;
      travel.keys.add('r' + b[6] + 'c' + b[7]);
      travel.max = Math.max(travel.max, b[8] | (b[9] << 8));
      return;
    }
    const key = b[0].toString(16) + '/' + b[1].toString(16) + '/' + b[5].toString(16);
    const e = (incoming[key] = incoming[key] || { n: 0, first: t, examples: [] });
    e.n++;
    if (e.examples.length < 3) e.examples.push(t + 'ms  ' + trim(b));
  };

  // Outgoing: wrap the report-sending methods once, for every device.
  for (const name of ['sendReport', 'sendFeatureReport']) {
    const orig = HIDDevice.prototype[name];
    originals.push([name, orig]);
    HIDDevice.prototype[name] = function (id, data) {
      try {
        recordOut(name === 'sendReport' ? id : 'f' + id, data);
      } catch (e) {
        console.warn('fizz recorder:', e);
      }
      return orig.apply(this, arguments);
    };
  }

  // Incoming: listen on every device the page already has permission for.
  navigator.hid.getDevices().then((devices) => {
    for (const d of devices) {
      const fn = (e) => {
        try {
          recordIn(e.reportId, e.data);
        } catch (err) {
          console.warn('fizz recorder:', err);
        }
      };
      d.addEventListener('inputreport', fn);
      listeners.push([d, fn]);
    }
    console.log('fizz recorder: listening on', devices.length, 'device(s):',
      devices.map((d) => d.productName + ' ' + d.vendorId.toString(16) + ':' + d.productId.toString(16)).join(', '));
  });

  window.__fizz = {
    dump() {
      const lines = [];
      lines.push('# Sent by the page (time in ms since the recorder started; runs of identical packets merged)');
      for (const o of outgoing) {
        lines.push(`${o.first}${o.n > 1 ? '..' + o.last : ''}ms  x${o.n}  id=${o.id}  ${o.hex}`);
      }
      lines.push('');
      lines.push('# Received, other than the key-travel stream (count, first time, up to 3 examples)');
      for (const [k, e] of Object.entries(incoming)) {
        lines.push(`${k}  x${e.n}  first ${e.first}ms`);
        for (const x of e.examples) lines.push('    ' + x);
      }
      lines.push('');
      lines.push(`# Key-travel stream (0x21 sub 1): ${travel.n} reports, max travel ${travel.max}`);
      lines.push('keys seen: ' + [...travel.keys].join(' '));
      lines.push('reports per second: ' + JSON.stringify(travel.perSecond));
      lines.push('first reports:');
      for (const x of travel.first) lines.push('    ' + x);
      return lines.join('\n');
    },
    stop() {
      for (const [name, orig] of originals) HIDDevice.prototype[name] = orig;
      for (const [d, fn] of listeners) d.removeEventListener('inputreport', fn);
      delete window.__fizz;
      console.log('fizz recorder removed');
    },
  };
  console.log('fizz recorder installed. Do the action, then run: copy(__fizz.dump())');
})();
