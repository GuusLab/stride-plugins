#!/usr/bin/env node
// Usage: node tools/gallery.mjs
//
// Renders site/index.html (the store gallery) and the plugin table in
// README.md from site/index.json. Run it after `stride plugin index`.
import { readFile, writeFile } from 'node:fs/promises';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const REPO = 'https://github.com/GuusLab/stride-plugins';
const index = JSON.parse(await readFile(join(root, 'site', 'index.json'), 'utf8'));
const plugins = index.plugins;
const esc = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]);
const rel = (url) => url.replace(/^https:\/\/[^/]+\/[^/]+\//, '');
const latest = (p) => p.releases.find((r) => r.version === p.latest);

const card = (p) => {
  const r = latest(p);
  const paras = (p.longDescription || p.description).split(/\n\s*\n/).map((t) => `<p>${esc(t)}</p>`).join('');
  const shots = (p.screenshots ?? []).map((s) => `
          <figure><a href="${esc(rel(s.url))}"><img src="${esc(rel(s.url))}" alt="${esc(s.caption)}" width="1600" height="1000" loading="lazy"></a><figcaption>${esc(s.caption)}</figcaption></figure>`).join('');
  return `
    <article class="plugin" id="${esc(p.id)}" style="--accent:${esc(p.accent || '#475569')}">
      <header>
        ${p.icon ? `<img class="icon" src="${esc(rel(p.icon.url))}" alt="" width="96" height="96">` : `<span class="icon tile">${esc(p.name[0])}</span>`}
        <div>
          <h2>${esc(p.name)}</h2>
          <p class="tagline">${esc(p.tagline || p.description)}</p>
          <p class="meta">
            ${(p.categories ?? []).map((c) => `<span class="chip">${esc(c)}</span>`).join('')}
            <span>v${esc(p.latest)}</span> · <span>${esc(p.publisher.name)}</span>
          </p>
        </div>
      </header>
      <div class="shots">${shots}
      </div>
      <details>
        <summary>About ${esc(p.name)}</summary>
        ${paras}
        <dl>
          <dt>Permissions</dt><dd>${r.permissions.map((x) => `<code>${esc(x)}</code>`).join(' ') || 'none'}</dd>
          <dt>Hooks</dt><dd>${r.hooks.map((x) => `<code>${esc(x)}</code>`).join(' ') || 'none'}</dd>
          <dt>Module</dt><dd><a href="${esc(rel(r.url))}">plugin.wasm</a>, ${(r.size / 1024).toFixed(0)} KB, SHA-256 <code class="hash">${esc(r.contentHash)}</code></dd>
          <dt>Source</dt><dd><a href="${REPO}/tree/${esc(r.sourceCommit)}/sources/${esc(p.id)}">sources/${esc(p.id)}</a> at <code>${esc(r.sourceCommit.slice(0, 7))}</code></dd>
        </dl>
      </details>
    </article>`;
};

const html = `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Stride Plugins</title>
<meta name="description" content="The official plugin store for Stride: sandboxed, signed WebAssembly plugins, rebuilt from source.">
<style>
:root { --bg:#f8fafc; --card:#fff; --text:#0f172a; --muted:#475569; --line:#e2e8f0; --link:#1d4ed8; color-scheme: light dark; }
@media (prefers-color-scheme: dark) { :root { --bg:#0b1120; --card:#111827; --text:#e5e7eb; --muted:#9ca3af; --line:#1f2937; --link:#93c5fd; } }
* { box-sizing: border-box; }
body { margin:0; background:var(--bg); color:var(--text); font:16px/1.55 system-ui, -apple-system, "Segoe UI", sans-serif; }
a { color:var(--link); }
.wrap { max-width:1100px; margin:0 auto; padding:0 16px; }
.top { padding:48px 0 24px; }
.top h1 { font-size:clamp(28px,5vw,40px); margin:0 0 8px; letter-spacing:-.02em; }
.top p { color:var(--muted); margin:0 0 8px; max-width:62ch; }
.plugin { background:var(--card); border:1px solid var(--line); border-top:4px solid var(--accent); border-radius:16px; padding:20px; margin:0 0 20px; }
.plugin header { display:flex; gap:16px; align-items:center; }
.icon { width:72px; height:72px; border-radius:18px; flex:none; }
.tile { display:grid; place-items:center; background:var(--accent); color:#fff; font-size:32px; font-weight:700; }
h2 { margin:0; font-size:22px; }
.tagline { margin:2px 0 6px; }
.meta { margin:0; color:var(--muted); font-size:14px; display:flex; flex-wrap:wrap; gap:6px; align-items:center; }
.chip { border:1px solid var(--line); border-radius:999px; padding:0 10px; }
.shots { display:flex; gap:12px; overflow-x:auto; padding:16px 0 8px; scroll-snap-type:x mandatory; }
.shots figure { margin:0; flex:0 0 min(420px, 80%); scroll-snap-align:start; }
.shots img { width:100%; height:auto; border-radius:10px; border:1px solid var(--line); display:block; }
figcaption { font-size:13px; color:var(--muted); margin-top:6px; }
details summary { cursor:pointer; font-weight:600; padding:6px 0; }
dl { display:grid; grid-template-columns:max-content 1fr; gap:4px 16px; font-size:14px; }
dt { color:var(--muted); } dd { margin:0; min-width:0; }
.hash { word-break:break-all; }
footer { color:var(--muted); font-size:14px; padding:24px 0 48px; }
@media (max-width:520px) { .icon { width:56px; height:56px; } dl { grid-template-columns:1fr; } }
</style>
</head>
<body>
<main class="wrap">
  <div class="top">
    <h1>Stride Plugins</h1>
    <p>The official plugin store for <a href="https://github.com/GuusLab/stride">Stride</a>. Every plugin is WebAssembly, sandboxed, permission-scoped, signed, and rebuilt from source byte for byte before it is listed.</p>
    <p>Install them from <strong>Plugins</strong> in the Stride editor. ${plugins.length} plugins · <a href="index.json">index.json</a> · <a href="${REPO}">GitHub</a> · <a href="${REPO}#submitting-a-plugin">Submit a plugin</a></p>
  </div>
${plugins.map(card).join('\n')}
  <footer>Index generated ${esc(index.generatedAt)}, signed by the registry key. Source and submissions: <a href="${REPO}">${REPO.replace('https://', '')}</a>.</footer>
</main>
</body>
</html>
`;
await writeFile(join(root, 'site', 'index.html'), html);

// README table
const rows = plugins.map((p) => {
  const perms = latest(p).permissions.map((x) => `\`${x}\``).join(', ') || 'none';
  return `| <img src="plugins/${p.id}/icon.png" width="40" alt=""> | [**${p.name}**](sources/${p.id}) | ${(p.tagline || p.description).replace(/\|/g, '\\|')} | ${perms} |`;
});
const readmePath = join(root, 'README.md');
const readme = await readFile(readmePath, 'utf8');
const table = ['| | Plugin | What it does | Permissions |', '|---|---|---|---|', ...rows].join('\n');
const next = readme.replace(/\| \| Plugin \| What it does \| Permissions \|\n(\|.*\|\n)+/, table + '\n');
if (next === readme && !readme.includes(table)) throw new Error('README plugin table not found');
await writeFile(readmePath, next);
console.log(`wrote site/index.html and the README table (${plugins.length} plugins)`);
