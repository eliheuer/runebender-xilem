// Capture the actual menu colors and header layout at wide and narrow sizes.
const fs = require('node:fs');
const path = require('node:path');
const { chromium } = require(process.env.RUNEBENDER_PLAYWRIGHT || 'playwright');
const output = process.env.RUNEBENDER_PROOFS;
if (!output) throw new Error('Set RUNEBENDER_PROOFS to a screenshot output directory');
fs.mkdirSync(output, { recursive: true });
(async () => {
  const browser = await chromium.launch({ headless: true, executablePath: process.env.RUNEBENDER_CHROME });
  try {
    const page = await browser.newPage({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 2 });
    await page.goto(process.env.RUNEBENDER_DEMO_URL || 'http://127.0.0.1:4326/');
    const frame = process.env.RUNEBENDER_IFRAME ? page.frames().find(f => f.url().includes('/app/index.html')) : page;
    await frame.waitForFunction(() => window.runebender);
    const setTheme = async theme => {
      await page.mouse.click(393, 15);
      await page.mouse.click(515, 284);
      await page.mouse.click(681, { dark: 43, gray: 66, light: 92 }[theme]);
    };
    for (const theme of ['gray', 'light', 'dark']) {
      await setTheme(theme);
      await page.waitForTimeout(150);
      for (const [menu, x] of [['filter', 345], ['file', 112]]) {
        await page.mouse.click(x, 15);
        await page.waitForTimeout(100);
        await page.screenshot({ path: path.join(output, `${menu}-${theme}-2x.png`), clip: { x: 0, y: 0, width: 1440, height: 300 } });
        await page.keyboard.press('Escape');
      }
    }
    await page.mouse.dblclick(410, 150, { delay: 100 });
    for (const width of [1000, 1280, 1920]) {
      await page.setViewportSize({ width, height: 800 }); await page.waitForTimeout(150);
      await page.screenshot({ path: path.join(output, `editor-${width}-2x.png`), clip: { x: 0, y: 0, width, height: 150 } });
    }
  } finally { await browser.close(); }
})().catch(error => { console.error(error); process.exit(1); });
