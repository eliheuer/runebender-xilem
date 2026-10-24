// Fonts on disk through the browser host: open a folder's files, edit, and save them back.
// The test hooks stand in for the File System Access folder picker.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { chromium } = require(process.env.RUNEBENDER_PLAYWRIGHT || 'playwright');
const fonts = process.env.RUNEBENDER_TEST_FONTS || path.join(__dirname, '../../virtua-grotesk/sources');
function readTree(root, directory, out) {
  for (const name of fs.readdirSync(directory).sort()) {
    if (name.startsWith('.')) continue;
    const full = path.join(directory, name);
    if (fs.statSync(full).isDirectory()) readTree(root, full, out);
    else out.push([path.relative(root, full).split(path.sep).join('/'), [...fs.readFileSync(full)]]);
  }
}
(async () => {
  const files = [];
  for (const name of fs.readdirSync(fonts)) {
    const full = path.join(fonts, name);
    if (name.endsWith('.designspace')) files.push([name, [...fs.readFileSync(full)]]);
    else if (name.toLowerCase().endsWith('.ufo')) readTree(fonts, full, files);
  }
  assert.ok(files.some(([name]) => name.endsWith('.designspace')), `a designspace in ${fonts}`);
  const browser = await chromium.launch({ headless: true, executablePath: process.env.RUNEBENDER_CHROME });
  try {
    const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.goto(process.env.RUNEBENDER_DEMO_URL || 'http://127.0.0.1:4326/');
    await page.waitForFunction(() => window.runebender);
    const state = () => page.evaluate(() => window.runebender.state());
    assert.equal((await state()).source, 'VirtuaGrotesk-Regular.ufo', 'the demo font is open first');
    const error = await page.evaluate(files => window.runebender.openFiles(null, files.map(([p, b]) => [p, new Uint8Array(b)])), files);
    assert.equal(error, '', 'the folder opens');
    let opened = await state();
    assert.equal(opened.source, 'VirtuaGrotesk.designspace', 'the designspace is the document');
    assert.ok(opened.glyph_count > 800, `glyphs loaded: ${opened.glyph_count}`);
    assert.equal(opened.modified, false);
    await page.waitForTimeout(300);
    // Edit: open the first glyph cell, find a painted point marker, drag it.
    await page.mouse.dblclick(410, 150, { delay: 100 });
    await page.waitForTimeout(300);
    assert.equal((await state()).mode, 'editor', 'a glyph is open');
    const marker = await page.evaluate(() => {
      const c = document.querySelector('canvas'), s = devicePixelRatio;
      const x = Math.round(450 * s), y = Math.round(70 * s), w = Math.round(500 * s), h = Math.round(460 * s);
      const data = c.getContext('2d').getImageData(x, y, w, h).data;
      for (let py = 0; py < h; py++) for (let px = 0; px < w; px++) {
        const i = (py * w + px) * 4, [r, g, b] = data.subarray(i, i + 3);
        if (g > r * 1.25 && g > b * 1.15 && g > 80) return [(x + px + 1) / s, (y + py + 1) / s];
      }
      return null;
    });
    assert.ok(marker, 'an outline point marker is painted');
    const before = (await state()).points;
    await page.mouse.move(...marker); await page.mouse.down();
    await page.mouse.move(marker[0] + 24, marker[1] - 16, { steps: 12 }); await page.mouse.up();
    await page.waitForTimeout(200);
    const edited = await state();
    assert.notDeepEqual(edited.points, before, 'the drag moves a point');
    assert.equal(edited.modified, true, 'the edit marks the document modified');
    const saved = await page.evaluate(() => {
      const bundle = window.runebender.saveFiles();
      return bundle && { prune: bundle.prune, files: bundle.files.map(([p, b]) => [p, b.length]) };
    });
    assert.ok(saved, 'a save bundle is produced');
    const written = new Set(saved.files.map(([p]) => p));
    const original = new Set(files.map(([p]) => p).filter(p => !p.endsWith('.designspace')));
    assert.deepEqual([...written].sort(), [...original].sort(), 'the saved file set matches the opened one');
    assert.ok(saved.prune.includes('VirtuaGrotesk-Regular.ufo/glyphs'), 'glyph directories are pruned');
    await page.evaluate(() => window.runebender.saved());
    await page.waitForTimeout(100);
    assert.equal((await state()).modified, false, 'a reported save clears the modified flag');
    assert.deepEqual(errors, [], 'no page errors');
    console.log(`folder check passed: ${opened.glyph_count} glyphs, ${written.size} files`);
  } finally {
    await browser.close();
  }
})().catch(error => { console.error(error); process.exit(1); });
