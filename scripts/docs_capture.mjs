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

async function cleanOldMedia() {
  const files = await fs.readdir(screenshotsDir).catch(() => []);
  for (const file of files) {
    if (file.endsWith('.png')) {
      await fs.rm(path.join(screenshotsDir, file), { force: true });
    }
  }
  const mediaFiles = await fs.readdir(mediaDir).catch(() => []);
  for (const file of mediaFiles) {
    if (file.endsWith('.webm')) {
      await fs.rm(path.join(mediaDir, file), { force: true });
    }
  }
}

async function clickByText(page, text) {
  const candidates = [
    page.getByRole('button', { name: text, exact: false }).first(),
    page.locator(`button:has-text("${text}")`).first(),
    page.locator(`summary:has-text("${text}")`).first()
  ];
  for (const el of candidates) {
    const count = await el.count();
    if (count > 0) {
      await el.waitFor({ state: 'visible', timeout: 10000 });
      await el.click();
      return;
    }
  }
  throw new Error(`Could not find clickable control with text: ${text}`);
}

async function stableScreenshot(page, fileName) {
  await page.waitForTimeout(900);
  await page.screenshot({
    path: path.join(screenshotsDir, fileName),
    fullPage: true
  });
}

async function run() {
  await ensureDirs();
  await cleanOldMedia();

  const browser = await chromium.launch({ headless: true });
  const context = await browser.newContext({
    viewport: { width: 1560, height: 980 },
    recordVideo: { dir: mediaDir, size: { width: 1280, height: 720 } }
  });
  const page = await context.newPage();
  page.setDefaultTimeout(15000);

  await page.goto(baseUrl, { waitUntil: 'domcontentloaded' });
  await page.waitForTimeout(1800);

  await stableScreenshot(page, '01-main-session.png');

  await clickByText(page, 'Auth & Channel');
  await stableScreenshot(page, '02-auth-channel.png');

  await clickByText(page, 'Cloud AI');
  await stableScreenshot(page, '03-cloud-ai.png');

  await clickByText(page, 'Settings');
  await clickByText(page, 'Diagnostics, Self-Test, and Debug Export');
  await page.waitForTimeout(600);
  await stableScreenshot(page, '04-settings.png');

  await clickByText(page, 'About');
  await stableScreenshot(page, '05-about.png');

  await context.close();
  await browser.close();

  const files = await fs.readdir(mediaDir);
  const webm = files.find((file) => file.endsWith('.webm'));
  if (webm) {
    const from = path.join(mediaDir, webm);
    const to = path.join(mediaDir, 'walkthrough.webm');
    if (from !== to) {
      await fs.copyFile(from, to);
      await fs.rm(from, { force: true });
    }
  }
}

run().catch((err) => {
  console.error(err);
  process.exitCode = 1;
});
