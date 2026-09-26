#!/usr/bin/env node
// Usage: node tools/verify.mjs [--no-build]
//
// What CI runs. For every sources/<id>: rebuilds it with tools/build-plugin.sh
// and checks the SHA-256 equals the contentHash of the latest release in
// plugins/<id>/plugin.json (skipped while there is no submission yet). For
// every plugins/<id>: checks the store metadata and images (PNG only, icon
// 512x512 <= 256 KB, 2-5 screenshots 1600x1000 <= 1.5 MB each).
import { readFile, readdir, stat } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const build = !process.argv.includes('--no-build');
const CATEGORIES = ['content', 'marketing', 'seo', 'social', 'commerce', 'privacy', 'design', 'analytics', 'developer', 'utilities'];
const errors = [];
const fail = (msg) => { errors.push(msg); console.error(`FAIL ${msg}`); };

const dirs = async (p) => existsSync(p) ? (await readdir(p, { withFileTypes: true })).filter((d) => d.isDirectory()).map((d) => d.name) : [];
const cmpVer = (a, b) => {
  const pa = a.split(/[.+-]/).map(Number), pb = b.split(/[.+-]/).map(Number);
  for (let i = 0; i < 3; i++) if ((pa[i] || 0) !== (pb[i] || 0)) return (pa[i] || 0) - (pb[i] || 0);
  return 0;
};
function releasesOf(sub) {
  const list = [];
  if (Array.isArray(sub.releases)) list.push(...sub.releases);
  if (Array.isArray(sub.release)) list.push(...sub.release);
  else if (sub.release) list.push(sub.release);
  return list;
}
async function png(path, w, h, max, label) {
  if (!existsSync(path)) return fail(`${label}: missing ${path}`);
  const buf = await readFile(path);
  const sig = '89504e470d0a1a0a';
  if (buf.subarray(0, 8).toString('hex') !== sig || buf.subarray(12, 16).toString() !== 'IHDR') return fail(`${label}: not a PNG`);
  const pw = buf.readUInt32BE(16), ph = buf.readUInt32BE(20);
  if (pw !== w || ph !== h) fail(`${label}: ${pw}x${ph}, expected ${w}x${h}`);
  if (buf.length > max) fail(`${label}: ${buf.length} bytes, max ${max}`);
  console.log(`ok   ${label} ${pw}x${ph} ${buf.length} B`);
}

// 1. Rebuild every source and compare hashes.
for (const id of await dirs(join(root, 'sources'))) {
  const subPath = join(root, 'plugins', id, 'plugin.json');
  if (!existsSync(subPath)) { console.log(`skip sources/${id}: no submission yet`); continue; }
  const sub = JSON.parse(await readFile(subPath, 'utf8'));
  const rels = releasesOf(sub).filter((r) => !r.yanked);
  if (!rels.length) { console.log(`skip sources/${id}: no release`); continue; }
  const latest = rels.sort((a, b) => cmpVer(a.version, b.version)).at(-1);
  if (build) execFileSync(join(root, 'tools', 'build-plugin.sh'), [id], { stdio: 'inherit' });
  const wasm = join(root, 'sources', id, 'plugin.wasm');
  if (!existsSync(wasm)) { fail(`sources/${id}: no plugin.wasm`); continue; }
  const hash = createHash('sha256').update(await readFile(wasm)).digest('hex');
  if (hash !== latest.contentHash) fail(`sources/${id}: rebuilt ${hash} != contentHash ${latest.contentHash} (${latest.version})`);
  else console.log(`ok   sources/${id} ${latest.version} ${hash}`);
  const manifest = await readFile(join(root, 'sources', id, 'stride-plugin.json'));
  const mh = createHash('sha256').update(manifest).digest('hex');
  if (latest.manifestHash && mh !== latest.manifestHash) fail(`sources/${id}: stride-plugin.json ${mh} != manifestHash ${latest.manifestHash}`);
}

// 2. Store metadata and images.
for (const id of await dirs(join(root, 'plugins'))) {
  const dir = join(root, 'plugins', id);
  const subPath = join(dir, 'plugin.json');
  if (!existsSync(subPath)) { fail(`plugins/${id}: no plugin.json`); continue; }
  let sub;
  try { sub = JSON.parse(await readFile(subPath, 'utf8')); } catch (e) { fail(`plugins/${id}/plugin.json: ${e.message}`); continue; }
  const s = sub.store;
  if (!s) { console.log(`note plugins/${id}: no store metadata`); continue; }
  if (s.tagline !== undefined && (typeof s.tagline !== 'string' || s.tagline.length > 80)) fail(`plugins/${id}: tagline must be <= 80 chars`);
  if (s.longDescription !== undefined && (typeof s.longDescription !== 'string' || s.longDescription.length > 4000)) fail(`plugins/${id}: longDescription must be <= 4000 chars`);
  if (s.accent !== undefined && !/^#[0-9A-Fa-f]{6}$/.test(s.accent)) fail(`plugins/${id}: accent must be #RRGGBB`);
  for (const c of s.categories ?? []) if (!CATEGORIES.includes(c)) fail(`plugins/${id}: unknown category ${c}`);
  if (s.icon !== undefined) {
    if (s.icon !== 'icon.png') fail(`plugins/${id}: icon must be "icon.png"`);
    await png(join(dir, 'icon.png'), 512, 512, 256 * 1024, `plugins/${id}/icon.png`);
  }
  const shots = s.screenshots ?? [];
  if (shots.length && (shots.length < 2 || shots.length > 5)) fail(`plugins/${id}: 2-5 screenshots, got ${shots.length}`);
  for (const [i, shot] of shots.entries()) {
    if (!/^screenshots\/\d\d\.png$/.test(shot.file ?? '')) fail(`plugins/${id}: screenshot ${i} file must be screenshots/NN.png`);
    if (typeof shot.caption !== 'string' || shot.caption.length > 120) fail(`plugins/${id}: screenshot ${i} caption must be <= 120 chars`);
    if (shot.file) await png(join(dir, shot.file), 1600, 1000, 1.5 * 1024 * 1024, `plugins/${id}/${shot.file}`);
  }
}

// 3. Any PNG anywhere under plugins/ or site/plugins/ must obey the limits too.
async function walk(p) {
  if (!existsSync(p)) return [];
  const out = [];
  for (const d of await readdir(p, { withFileTypes: true })) {
    const f = join(p, d.name);
    if (d.isDirectory()) out.push(...await walk(f)); else if (d.name.endsWith('.png')) out.push(f);
  }
  return out;
}
for (const f of [...await walk(join(root, 'plugins')), ...await walk(join(root, 'site', 'plugins'))]) {
  const rel = f.slice(root.length + 1);
  if (/\/icon\.png$/.test(f)) await png(f, 512, 512, 256 * 1024, rel);
  else if (/\/screenshots\/\d\d\.png$/.test(f)) await png(f, 1600, 1000, 1.5 * 1024 * 1024, rel);
  else fail(`${rel}: unexpected PNG (only icon.png and screenshots/NN.png are allowed)`);
}

if (errors.length) { console.error(`\n${errors.length} problem(s)`); process.exit(1); }
console.log('\nall checks passed');
