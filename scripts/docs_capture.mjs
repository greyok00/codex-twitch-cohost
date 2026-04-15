import fs from 'node:fs/promises';
import path from 'node:path';
import { chromium } from 'playwright';

const candidateUrls = process.env.DOCS_CAPTURE_URL
  ? [process.env.DOCS_CAPTURE_URL]
  : [
      'http://127.0.0.1:1420/',
      'http://127.0.0.1:4180/',
      'http://127.0.0.1:5173/'
    ];
const screenshotsDir = path.resolve('docs/screenshots');
const heroScreenshot = path.join(screenshotsDir, 'hero-grey-ui.png');

async function ensureDirs() {
  await fs.mkdir(screenshotsDir, { recursive: true });
}

async function stableScreenshot(page) {
  await page.waitForTimeout(2200);
  await page.evaluate(() => {
    for (const node of document.querySelectorAll('.banner-error')) node.remove();
    const startup = document.querySelector('.startup-overlay');
    if (startup) startup.remove();
  });
  await page.screenshot({
    path: heroScreenshot,
    fullPage: false
  });
}

async function openFirstReachable(page) {
  for (const url of candidateUrls) {
    try {
      await page.goto(url, { waitUntil: 'domcontentloaded', timeout: 10000 });
      return;
    } catch (err) {
      if (url === candidateUrls[candidateUrls.length - 1]) throw err;
    }
  }
}

async function run() {
  await ensureDirs();
  await fs.rm(heroScreenshot, { force: true });

  const browser = await chromium.launch({ headless: true });
  const context = await browser.newContext({
    viewport: { width: 1680, height: 1180 },
    deviceScaleFactor: 1
  });
  const page = await context.newPage();
  page.setDefaultTimeout(20000);

  await openFirstReachable(page);
  await page.getByText('STT Tracking').waitFor({ state: 'visible' });
  await page.getByText('Chat').first().waitFor({ state: 'visible' });
  await stableScreenshot(page);

  await context.close();
  await browser.close();
}

run().catch((err) => {
  console.error(err);
  process.exitCode = 1;
});
