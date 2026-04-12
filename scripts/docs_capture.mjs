import fs from 'node:fs/promises';
import path from 'node:path';
import { chromium } from 'playwright';

const baseUrl = process.env.DOCS_CAPTURE_URL || 'http://127.0.0.1:5173/';
const screenshotsDir = path.resolve('docs/screenshots');
const mediaDir = path.resolve('docs/media');

async function ensureDirs() {
  await fs.mkdir(screenshotsDir, { recursive: true });
  await fs.mkdir(mediaDir, { recursive: true });
}

async function clickByText(page, text) {
  const candidates = [
    page.getByRole('button', { name: text, exact: false }).first(),
    page.locator(`button:has-text("${text}")`).first(),
    page.locator(`[role="tab"]:has-text("${text}")`).first()
  ];
  for (const el of candidates) {
    const count = await el.count();
    if (count > 0) {
      await el.waitFor({ state: 'visible', timeout: 8000 });
      await el.click();
      return;
    }
  }
  throw new Error(`Could not find clickable tab/button with text: ${text}`);
}

async function run() {
  await ensureDirs();
  const browser = await chromium.launch({ headless: true });
  const context = await browser.newContext({
    viewport: { width: 1560, height: 980 },
    recordVideo: { dir: mediaDir, size: { width: 1280, height: 720 } }
  });
  const page = await context.newPage();
  page.setDefaultTimeout(12000);

  await page.goto(baseUrl, { waitUntil: 'domcontentloaded' });
  await page.waitForTimeout(2000);

  await page.screenshot({ path: path.join(screenshotsDir, '01-main-session.png'), fullPage: true });

  await clickByText(page, 'Auth & Channel');
  await page.waitForTimeout(900);
  await page.screenshot({ path: path.join(screenshotsDir, '02-auth-channel.png'), fullPage: true });

  await clickByText(page, 'AI Setup');
  await page.waitForTimeout(900);
  await page.screenshot({ path: path.join(screenshotsDir, '03-ai-setup.png'), fullPage: true });

  await clickByText(page, 'Voice Input');
  await page.waitForTimeout(900);
  await page.screenshot({ path: path.join(screenshotsDir, '04-voice-input.png'), fullPage: true });

  await clickByText(page, 'Diagnostics');
  await page.waitForTimeout(900);
  await page.screenshot({ path: path.join(screenshotsDir, '05-diagnostics.png'), fullPage: true });

  await clickByText(page, 'Memory');
  await page.waitForTimeout(900);
  await page.screenshot({ path: path.join(screenshotsDir, '06-memory.png'), fullPage: true });

  await clickByText(page, 'About');
  await page.waitForTimeout(900);
  await page.screenshot({ path: path.join(screenshotsDir, '07-about.png'), fullPage: true });

  await context.close();
  await browser.close();

  const files = await fs.readdir(mediaDir);
  const webm = files.find((file) => file.endsWith('.webm'));
  if (webm) {
    const from = path.join(mediaDir, webm);
    const to = path.join(mediaDir, 'walkthrough.webm');
    if (from !== to) {
      await fs.copyFile(from, to);
    }
  }
}

run().catch((err) => {
  console.error(err);
  process.exitCode = 1;
});
