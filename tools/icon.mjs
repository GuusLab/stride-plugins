#!/usr/bin/env node
// Usage: node tools/icon.mjs <in.svg> <out.png> [width height]
// Renders an SVG to a PNG with Playwright's Chromium. Default 512x512 (store
// icon); pass 1600 1000 to render a screenshot-sized SVG.
import { chromium } from 'playwright';
import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';

const [input, output, w = '512', h = '512'] = process.argv.slice(2);
if (!input || !output) {
  console.error('usage: node tools/icon.mjs <in.svg> <out.png> [width height]');
  process.exit(2);
}
const width = Number(w), height = Number(h);
const svg = await readFile(resolve(input), 'utf8');
const browser = await chromium.launch();
try {
  const page = await browser.newPage({ viewport: { width, height }, deviceScaleFactor: 1 });
  await page.setContent(
    `<!doctype html><html><head><style>html,body{margin:0;padding:0;background:transparent}
     svg{display:block;width:${width}px;height:${height}px}</style></head><body>${svg}</body></html>`,
  );
  await page.screenshot({ path: resolve(output), omitBackground: true, clip: { x: 0, y: 0, width, height } });
  console.log(`${output} ${width}x${height}`);
} finally {
  await browser.close();
}
