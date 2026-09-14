// Exercise actual menu input and assert that point markers remain painted.
const assert = require('node:assert/strict');
const { chromium } = require(process.env.RUNEBENDER_PLAYWRIGHT || 'playwright');
(async () => {
  const browser = await chromium.launch({ headless: true, executablePath: process.env.RUNEBENDER_CHROME });
  try {
    const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.goto(process.env.RUNEBENDER_DEMO_URL || 'http://127.0.0.1:4324/');
    const frame = process.env.RUNEBENDER_IFRAME ? page.frames().find(f => f.url().includes('/app/index.html')) : page;
    assert.ok(frame);
    await frame.waitForFunction(() => window.runebender);
    await page.mouse.click(70, 120);
    await page.keyboard.type('ampersand', { delay: 30 });
    await page.mouse.dblclick(306, 150, { delay: 100 });
    assert.equal(await frame.evaluate(() => window.runebender.state().glyph), 'ampersand');
    for (const [theme, y] of [['Light', 92], ['Dark', 43]]) {
      await page.mouse.click(440, 15);
      await page.mouse.click(515, 284);
      await page.mouse.click(681, y);
      // No pointer movement over the outline may trigger the missing repaint.
      await page.waitForTimeout(150);
      const marks = await frame.evaluate(() => {
        const data = document.querySelector('#app').getContext('2d').getImageData(500, 100, 250, 330).data;
        let count = 0;
        for (let i = 0; i < data.length; i += 4) {
          const [r, g, b] = data.subarray(i, i + 3);
          if ((g > r * 1.1 && g > b * 1.1) || (b > r * 1.05 && r > g * 1.05)) count++;
        }
        return count;
      });
      assert.ok(marks > 50, `${theme}: outline point markers remain visible (${marks} colored pixels)`);
      console.log(`${theme}: ${marks} marker pixels remain visible`);
    }
    assert.deepEqual(errors, []);
  } finally { await browser.close(); }
})().catch(error => { console.error(error); process.exit(1); });
