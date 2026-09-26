#!/usr/bin/env node
// Usage: node tools/install-local.mjs <port> <sourceDir>
//
// Against a server started by tools/demo-server.sh: logs in as the demo owner,
// sideloads <sourceDir>/stride-plugin.json + <sourceDir>/plugin.wasm through
// plugins.install, granting every permission the manifest asks for, and
// enables the plugin. Re-running uninstalls nothing; if the plugin is already
// installed the error is printed and the script still tries to enable it.
import { readFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';

const [port, dir] = process.argv.slice(2);
if (!port || !dir) {
  console.error('usage: node tools/install-local.mjs <port> <sourceDir>');
  process.exit(2);
}
const base = `http://127.0.0.1:${port}`;
const email = process.env.STRIDE_DEMO_EMAIL ?? 'demo@stride.test';
const password = process.env.STRIDE_DEMO_PASSWORD ?? 'stride-demo-password-2026';

let cookie = '';
async function post(path, body) {
  const res = await fetch(base + path, {
    method: 'POST',
    headers: { 'content-type': 'application/json', ...(cookie ? { cookie } : {}) },
    body: JSON.stringify(body),
  });
  const set = res.headers.getSetCookie?.() ?? [];
  if (set.length) cookie = set.map((c) => c.split(';')[0]).join('; ');
  const text = await res.text();
  let data;
  try { data = text ? JSON.parse(text) : null; } catch { data = text; }
  return { ok: res.ok, status: res.status, data };
}

const login = await post('/api/login', { email, password });
if (!login.ok) { console.error('login failed', login.status, login.data); process.exit(1); }

const src = resolve(dir);
const manifestText = await readFile(join(src, 'stride-plugin.json'), 'utf8');
const manifest = JSON.parse(manifestText);
const wasm = await readFile(join(src, 'plugin.wasm'));
const grant = manifest.permissions ?? [];

const install = await post('/api/actions/plugins.install', {
  manifest: manifestText,
  module: wasm.toString('base64'),
  grant,
  sideload: true,
});
if (install.ok) console.log(`installed ${manifest.id}@${manifest.version} granting [${grant.join(', ')}]`);
else console.error(`install answered ${install.status}:`, JSON.stringify(install.data));

// Make sure the grant is exactly what the manifest asks for (covers re-runs).
const perms = await post('/api/actions/plugins.permissions', { id: manifest.id, grant });
if (!perms.ok) { console.error('plugins.permissions failed', perms.status, JSON.stringify(perms.data)); process.exit(1); }

const enable = await post('/api/actions/plugins.enable', { id: manifest.id });
if (!enable.ok) { console.error('enable failed', enable.status, JSON.stringify(enable.data)); process.exit(1); }
console.log(`enabled ${manifest.id}`);
