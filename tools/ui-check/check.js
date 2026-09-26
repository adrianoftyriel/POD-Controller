// Headless smoke test of the web UI: loads a patch, opens each panel and the
// all-banks grid, saves screenshots and reports page errors. Loads 08C into
// the POD's edit buffer (audible, nothing saved). See ../../designprompt.md.
//
//   cd tools/ui-check && npm i playwright@1 && npx playwright install --with-deps chromium
//   node check.js [http://localhost:8080] [outdir]
const { chromium } = require('playwright');

const base = process.argv[2] || 'http://localhost:8080';
const out = process.argv[3] || '.';
const PANELS = ['gate', 'wah', 'stomp', 'amp', 'comp', 'eq', 'volume', 'loop', 'mod', 'delay', 'reverb', 'variax'];

(async () => {
  const browser = await chromium.launch();
  const errors = [];
  for (const [name, viewport] of [['desktop', { width: 1400, height: 900 }], ['phone', { width: 390, height: 844 }]]) {
    const page = await browser.newPage({ viewport });
    page.on('pageerror', (e) => errors.push(`${name}: ${e.message}`));
    page.on('console', (m) => m.type() === 'error' && errors.push(`${name}: ${m.text()}`));
    page.on('dialog', (d) => d.dismiss());
    await page.goto(base + '/?bank=8');
    await page.waitForTimeout(1500);
    await page.screenshot({ path: `${out}/${name}-start.png` });
    await page.click('.patch-card[data-slot="30"]');
    await page.waitForTimeout(1500);
    for (const panel of PANELS) {
      const block = page.locator(`.chain-block[data-block="${panel}"] .chain-select`);
      if (await block.count() === 0) continue;
      await block.click();
      await page.waitForTimeout(200);
      await page.screenshot({ path: `${out}/${name}-${panel}.png`, fullPage: true });
    }
    await page.click('#btn-all-banks');
    await page.waitForTimeout(300);
    await page.screenshot({ path: `${out}/${name}-all-banks.png` });
    await page.close();
  }
  await browser.close();
  console.log(errors.length ? `ERRORS:\n${errors.join('\n')}` : 'no page errors');
  process.exit(errors.length ? 1 : 0);
})();
