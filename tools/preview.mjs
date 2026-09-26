#!/usr/bin/env node
// Usage: node tools/preview.mjs <id> <page.html> <out.html> [values.json] [slug]
//
// Runs sources/<id>/plugin.wasm's on_page_render over a page the way Stride
// does, with the six host functions answered in memory: storage is seeded from
// values.json (the keys a panel would have saved), actions are refused, logs
// go to stderr. Writes the rendered page to out.html. For store screenshots
// and for eyeballing a hook without a server; `stride plugin test` is still
// the check the registry runs.
import createPlugin from '@extism/extism';
import { readFile, writeFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const [id, pagePath, outPath, valuesPath, slug = 'home'] = process.argv.slice(2);
if (!id || !pagePath || !outPath) {
  console.error('usage: node tools/preview.mjs <id> <page.html> <out.html> [values.json] [slug]');
  process.exit(2);
}

const kv = new Map(
  Object.entries(valuesPath && existsSync(valuesPath) ? JSON.parse(await readFile(valuesPath, 'utf8')) : {}),
);
const answer = (ctx, value) => ctx.store(JSON.stringify(value));
const request = (ctx, offset) => JSON.parse(ctx.read(offset).text());
const host = {
  stride_kv_get: (ctx, off) => answer(ctx, { ok: kv.has(request(ctx, off).key) ? kv.get(request(ctx, off).key) : null }),
  stride_kv_set: (ctx, off) => { const r = request(ctx, off); kv.set(r.key, r.value); return answer(ctx, { ok: true }); },
  stride_kv_delete: (ctx, off) => answer(ctx, { ok: kv.delete(request(ctx, off).key) }),
  stride_kv_list: (ctx, off) => {
    const prefix = request(ctx, off).prefix ?? '';
    return answer(ctx, { ok: [...kv.keys()].filter((k) => k.startsWith(prefix)).sort() });
  },
  stride_action: (ctx) => answer(ctx, { error: { code: 'refused', message: 'no actions in a preview' } }),
  stride_log: (ctx, off) => { const r = request(ctx, off); console.error(`[${r.level}] ${r.message}`); return answer(ctx, { ok: true }); },
};

const wasm = await readFile(join(root, 'sources', id, 'plugin.wasm'));
const plugin = await createPlugin({ wasm: [{ data: wasm }] }, {
  useWasi: false,
  runInWorker: false,
  functions: { 'extism:host/user': host },
});
try {
  const html = await readFile(pagePath, 'utf8');
  const out = await plugin.call('on_page_render', JSON.stringify({ siteId: 'site-preview', slug, html }));
  await writeFile(outPath, JSON.parse(out.text()).html);
  console.log(`${outPath} (${id} on ${slug})`);
} finally {
  await plugin.close();
}
