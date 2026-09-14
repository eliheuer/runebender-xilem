// Real browser input, physical-pixel checks, and screenshots at desktop densities.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { chromium } = require(process.env.RUNEBENDER_PLAYWRIGHT || 'playwright');
const output = process.env.RUNEBENDER_PROOFS;
if (output) fs.mkdirSync(output, { recursive: true });
(async () => {
  const browser = await chromium.launch({ headless: true, executablePath: process.env.RUNEBENDER_CHROME });
  const reports = [];
  try {
    for (const dpr of (process.env.RUNEBENDER_DPRS || '1,2,1.25').split(',').map(Number)) {
      const page = await browser.newPage({ viewport: { width: 1280, height: 800 }, deviceScaleFactor: dpr });
      const errors = [];
      page.on('pageerror', error => errors.push(error.message));
      await page.goto(process.env.RUNEBENDER_DEMO_URL || 'http://127.0.0.1:4326/');
      const frame = process.env.RUNEBENDER_IFRAME ? page.frames().find(f => f.url().includes('/app/index.html')) : page;
      assert.ok(frame, 'editor frame exists');
      await frame.waitForFunction(() => window.runebender);
      const settle = () => page.waitForTimeout(120);
      const state = () => frame.evaluate(() => window.runebender.state());
      const metrics = () => frame.evaluate(() => window.runebender.metrics());
      const shot = async name => { if (output) await page.screenshot({ path: path.join(output, `${name}-${dpr}x.png`) }); };
      const backing = async () => {
        await frame.waitForFunction(() => {
          const c = document.querySelector('canvas'), r = c.getBoundingClientRect();
          return c.width === Math.round(r.width * devicePixelRatio) && c.height === Math.round(r.height * devicePixelRatio);
        });
        const sizes = await frame.evaluate(() => {
          const c = document.querySelector('canvas'), r = c.getBoundingClientRect();
          return { actual: [c.width, c.height], expected: [Math.round(r.width * devicePixelRatio), Math.round(r.height * devicePixelRatio)] };
        });
        assert.deepEqual(sizes.actual, sizes.expected, 'canvas uses every display pixel');
      };
      const markers = () => frame.evaluate(() => {
        const c = document.querySelector('canvas'), s = devicePixelRatio;
        const x = Math.round(450 * s), y = Math.round(70 * s), w = Math.round(500 * s), h = Math.round(460 * s);
        const data = c.getContext('2d').getImageData(x, y, w, h).data;
        let first, minY = Infinity, maxY = 0, count = 0;
        for (let py = 0; py < h; py++) for (let px = 0; px < w; px++) {
          const i = (py * w + px) * 4, [r, g, b] = data.subarray(i, i + 3);
          if (g > r * 1.25 && g > b * 1.15 && g > 80) {
            if (!first) first = [(x + px + 1) / s, (y + py + 1) / s];
            minY = Math.min(minY, py); maxY = Math.max(maxY, py); count++;
          }
        }
        return { first, height: (maxY - minY) / s, count };
      });
      await settle(); await backing();
      assert.equal((await state()).glyph_count, 863);
      assert.equal((await state()).simd, true, 'optimized WASM SIMD pipeline is compiled in');
      await shot('overview-gray');
      const idle = (await metrics()).frames;
      await page.waitForTimeout(250);
      assert.equal((await metrics()).frames, idle, 'no idle repaint loop');
      await page.mouse.dblclick(410, 150, { delay: 100 });
      await settle(); assert.equal((await state()).glyph, 'exclam');
      await shot('editor-gray');
      const original = (await state()).points, point = (await markers()).first;
      assert.ok(point, 'outline point markers are painted');
      await page.mouse.move(...point); await page.mouse.down();
      await page.mouse.move(point[0] + 24, point[1] - 16, { steps: 12 }); await page.mouse.up();
      await settle(); const edited = (await state()).points;
      assert.notDeepEqual(edited, original, 'drag changes actual font coordinates');
      await page.keyboard.press('Meta+z'); assert.deepEqual((await state()).points, original, 'Cmd-Z restores outline');
      await page.keyboard.press('Control+Shift+z'); assert.deepEqual((await state()).points, edited, 'redo restores edit');
      await settle();
      const beforeZoom = (await markers()).height;
      await page.mouse.move(720, 320); await page.mouse.wheel(0, -40); await settle();
      assert.ok((await markers()).height > beforeZoom * 1.05, 'wheel zoom changes painted outline size');
      await page.mouse.move(245, 350); await settle();
      assert.equal(await page.locator('canvas').count(), process.env.RUNEBENDER_IFRAME ? 0 : 1);
      await page.mouse.down(); await settle();
      assert.equal(await frame.locator('canvas').evaluate(c => c.style.cursor), 'ew-resize', 'active splitter forwards the native resize cursor');
      await page.mouse.move(305, 350, { steps: 12 }); await page.mouse.up(); await settle();
      const divider = x => frame.evaluate(x => {
        const s = devicePixelRatio, c = document.querySelector('canvas');
        const data = c.getContext('2d').getImageData(Math.floor(x * s), Math.round(140 * s), 1, Math.round(360 * s)).data;
        let dark = 0; for (let i = 0; i < data.length; i += 4) if (data[i] < 150) dark++;
        return dark / (data.length / 4);
      }, x);
      assert.ok(await divider(305) > .95, 'drag moves the painted divider by 60 logical pixels');
      await shot('resized-editor');
      await page.mouse.move(305, 350); await page.mouse.down();
      await page.mouse.move(245, 350, { steps: 12 }); await page.mouse.up(); await settle();
      assert.ok(await divider(245) > .95, 'drag restores the original panel width');
      for (const [theme, y] of [['light', 92], ['dark', 43], ['gray', 66]]) {
        await page.mouse.click(440, 15); await page.mouse.click(515, 284); await page.mouse.click(681, y); await settle();
        assert.ok((await markers()).count > 30, `${theme}: outline survives theme changes`);
        const surface = await frame.evaluate(() => document.querySelector('canvas').getContext('2d')
          .getImageData(Math.round(2 * devicePixelRatio), Math.round(35 * devicePixelRatio), 1, 1).data[0]);
        assert.ok(theme === 'light' ? surface > 200 : theme === 'dark' ? surface < 80 : surface > 100 && surface < 200,
          `${theme}: actual painted surface changes with the menu selection`);
        await shot(`editor-${theme}`);
      }
      // Losing focus must release temporary tools/modifiers instead of sticking in pan.
      await page.keyboard.down('Space');
      await frame.evaluate(() => window.dispatchEvent(new Event('blur')));
      await page.keyboard.up('Space');
      await page.mouse.click(750, 450); await settle();
      await page.mouse.click(1150, 15); await settle();
      assert.equal((await state()).mode, 'nodes');
      const graph = (await state()).nodes;
      assert.equal(graph.nodes.length, 4, 'Nodes starts with an editable example');
      assert.equal(graph.links.length, 2);
      const header = await frame.evaluate(() => {
        const c = document.querySelector('canvas'), s = devicePixelRatio;
        const x = Math.round(330 * s), y = Math.round(100 * s), w = Math.round(620 * s), h = Math.round(480 * s);
        const data = c.getContext('2d').getImageData(x, y, w, h).data;
        for (let py = 0; py < h; py++) for (let px = 0; px < w; px++) {
          const i = (py * w + px) * 4, [r, g, b] = data.subarray(i, i + 3);
          if (g > r * 1.25 && g > b * 1.15 && g > 80) return [(x + px) / s + 20, (y + py) / s + 10];
        }
      });
      assert.ok(header, 'Font node header is painted');
      await page.mouse.move(...header); await page.mouse.down();
      await page.mouse.move(header[0] + 40, header[1] - 25, { steps: 8 }); await page.mouse.up(); await settle();
      assert.notDeepEqual((await state()).nodes.nodes[0].pos, graph.nodes[0].pos, 'drag moves the actual node');
      assert.deepEqual((await state()).nodes.links, graph.links, 'links survive node movement');
      await shot('nodes');
      await page.mouse.click(900, 500); await page.keyboard.press('Escape'); await settle();
      assert.equal((await state()).mode, 'overview', 'Escape from the canvas opens Font overview');
      // Paste and IME events are dispatched into the DOM input bridge, never the model.
      for (const [kind, text] of [['paste', 'ampersand'], ['composition', 'exclam']]) {
        await page.mouse.click(70, 86); await settle(); await page.keyboard.press('Control+a');
        assert.equal(await frame.evaluate(() => document.activeElement.id), 'text-input', 'text field owns browser input focus');
        if (kind === 'paste') assert.equal(await frame.evaluate(() => {
          const event = new KeyboardEvent('keydown', { key: 'v', code: 'KeyV', ctrlKey: true, bubbles: true, cancelable: true });
          document.activeElement.dispatchEvent(event);
          return event.defaultPrevented;
        }), false, 'native paste shortcut is not cancelled');
        await frame.evaluate(({ kind, text }) => {
          const input = document.activeElement;
          if (kind === 'paste') {
            const data = new DataTransfer(); data.setData('text/plain', text);
            input.dispatchEvent(new ClipboardEvent('paste', { clipboardData: data, bubbles: true, cancelable: true }));
          } else {
            input.dispatchEvent(new CompositionEvent('compositionstart', { bubbles: true }));
            input.dispatchEvent(new CompositionEvent('compositionupdate', { data: text, bubbles: true }));
            input.dispatchEvent(new CompositionEvent('compositionend', { data: text, bubbles: true }));
          }
        }, { kind, text });
        await settle(); await shot(`${kind}-search`); await page.mouse.dblclick(306, 160, { delay: 100 }); await settle();
        assert.equal((await state()).glyph, text, `${kind} updates the real search field`);
        await page.mouse.click(900, 500); await page.keyboard.press('Escape'); await settle();
        assert.equal((await state()).mode, 'overview', 'Escape from the canvas opens Font overview');
      }
      for (const viewport of [{ width: 1000, height: 720 }, { width: 1440, height: 900 }]) {
        await page.setViewportSize(viewport); await settle(); await backing();
        await shot(`overview-${viewport.width}`);
      }
      const devtools = await page.context().newCDPSession(page);
      for (const density of [1.5, dpr]) {
        await devtools.send('Emulation.setDeviceMetricsOverride', {
          width: 1440, height: 900, deviceScaleFactor: density, mobile: false,
        });
        await settle(); await backing();
        assert.equal((await state()).scale, density, 'Masonry tracks a changed display density');
      }
      await devtools.detach();
      assert.deepEqual(errors, []);
      const m = await metrics(), times = m.renderMs.slice(2).sort((a, b) => a - b);
      reports.push({ dpr, frames: m.frames, medianMs: times[Math.floor(times.length / 2)], p95Ms: times[Math.floor(times.length * .95)] });
      console.log('PASS', reports.at(-1));
      await page.close();
    }
    if (output) fs.writeFileSync(path.join(output, 'metrics.json'), JSON.stringify(reports, null, 2) + '\n');
  } finally { await browser.close(); }
})().catch(error => { console.error(error); process.exit(1); });
