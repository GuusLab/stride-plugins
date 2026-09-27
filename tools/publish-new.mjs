#!/usr/bin/env node
// Usage: node tools/publish-new.mjs [<id>...]
//
// Signs and publishes plugins whose submission (plugins/<id>/plugin.json) has
// no release yet — all of them when no id is given. For each one it:
//
//   1. rebuilds sources/<id>/plugin.wasm exactly as the verify workflow does
//      (x86_64 Linux, checked out at CI's path, so Cargo's metadata hashes
//      match; on any other machine this runs in a linux/amd64 container);
//   2. signs the release with the publisher key (`stride plugin publish`),
//      refusing a key whose public half is not publishers/guuslab.json's;
//   3. writes the release into plugins/<id>/plugin.json and copies the
//      module, manifest and store images under site/;
// and then signs site/index.json with the registry key, renders the gallery
// and runs `node tools/verify.mjs --no-build`. It commits nothing: review the
// diff, commit and push.
//
// Needs a pushed, clean checkout (the release names the commit its source is
// in), the stride CLI (`npm i -g @guuslab/stride`; override with STRIDE_BIN,
// e.g. STRIDE_BIN="$(command -v stride)"), and Docker unless this is x86_64
// Linux. Keys are read where they
// are and never copied:
//   PUBLISHER_KEY  default ~/.stride/guuslab.key
//   REGISTRY_KEY   default ~/.stride/stride-registry.key
import { readFile, writeFile, mkdir, copyFile, readdir } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { homedir, arch, platform } from 'node:os';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const BASE = 'https://guuslab.github.io/stride-plugins';
const REPOSITORY = 'https://github.com/GuusLab/stride-plugins';
// Where actions/checkout puts this repository on GitHub's runners. The path of
// the vendored PDK is part of Cargo's crate metadata, so a build anywhere else
// produces different bytes.
const CI_CHECKOUT = '/home/runner/work/stride-plugins/stride-plugins';
const STRIDE = process.env.STRIDE_BIN ?? 'stride';
const PUBLISHER_KEY = process.env.PUBLISHER_KEY ?? join(homedir(), '.stride', 'guuslab.key');
const REGISTRY_KEY = process.env.REGISTRY_KEY ?? join(homedir(), '.stride', 'stride-registry.key');

const run = (cmd, args, opts = {}) => execFileSync(cmd, args, { cwd: root, encoding: 'utf8', ...opts });
const die = (msg) => { console.error(`\n${msg}`); process.exit(1); };
const readJson = async (p) => JSON.parse(await readFile(join(root, p), 'utf8'));
// Key order does not matter to a manifest; compare with sorted keys.
const canonical = (v) => JSON.stringify(v, (_, x) =>
  x && typeof x === 'object' && !Array.isArray(x) ? Object.fromEntries(Object.keys(x).sort().map((k) => [k, x[k]])) : x);

// --- preflight -------------------------------------------------------------
for (const [name, path] of [['PUBLISHER_KEY', PUBLISHER_KEY], ['REGISTRY_KEY', REGISTRY_KEY]]) {
  if (!existsSync(path)) die(`${name} not found at ${path}. Set ${name}=/path/to/key.`);
}
try { run(STRIDE, ['plugin'], { stdio: 'pipe' }); } catch (e) {
  if (!String(e.stderr ?? '').includes('plugin')) die(`"${STRIDE}" does not run. Install the CLI with "npm i -g @guuslab/stride", or set STRIDE_BIN to a stride with the plugin host.`);
}
if (run('git', ['status', '--porcelain']).trim()) die('The working tree has changes. Commit or stash them first.');
run('git', ['fetch', '--quiet', 'origin', 'main']);
const commit = run('git', ['rev-parse', 'HEAD']).trim();
if (!run('git', ['branch', '-r', '--contains', commit]).includes('origin/main')) {
  die(`HEAD (${commit.slice(0, 7)}) is not on origin/main. Push first: the release records the commit its source is in.`);
}

const publisher = await readJson('publishers/guuslab.json');
let ids = process.argv.slice(2);
if (!ids.length) {
  for (const id of (await readdir(join(root, 'plugins'))).sort()) {
    const path = join('plugins', id, 'plugin.json');
    if (!existsSync(join(root, path))) continue;
    const sub = await readJson(path);
    if (!sub.release && !sub.releases) ids.push(id);
  }
}
if (!ids.length) die('Nothing to publish: every submission already has a release.');
console.log(`Publishing ${ids.join(', ')} from ${commit.slice(0, 7)}`);

// --- 1. reference builds ---------------------------------------------------
if (platform() === 'linux' && arch() === 'x64' && root === CI_CHECKOUT) {
  for (const id of ids) run('tools/build-plugin.sh', [id], { stdio: 'inherit' });
} else {
  const channel = (await readFile(join(root, 'tools', 'rust-toolchain.toml'), 'utf8')).match(/channel *= *"(.*)"/)[1];
  const script = [
    'set -e',
    'cp -r /usr/local/cargo /home/runner/.cargo',
    'cp -r /usr/local/rustup /home/runner/.rustup',
    'export CARGO_HOME=/home/runner/.cargo RUSTUP_HOME=/home/runner/.rustup PATH=/home/runner/.cargo/bin:$PATH',
    'rustup target add wasm32-unknown-unknown >/dev/null',
    ...ids.map((id) => `rm -rf sources/${id}/target && tools/build-plugin.sh ${id}`),
  ].join('\n');
  run('docker', ['run', '--rm', '--platform', 'linux/amd64', '-v', `${root}:${CI_CHECKOUT}`, '-w', CI_CHECKOUT,
    `rust:${channel}`, 'sh', '-c', script], { stdio: 'inherit' });
}

// --- 2 and 3. sign each release and lay it out under site/ -------------------
for (const id of ids) {
  const subPath = join('plugins', id, 'plugin.json');
  const sub = await readJson(subPath);
  const out = run(STRIDE, ['plugin', 'publish', join('sources', id), '--key', PUBLISHER_KEY,
    '--base-url', BASE, '--repository', REPOSITORY, '--commit', commit]);
  const signed = JSON.parse(out.split('\n# release digest')[0]);
  if (signed.publisher.publicKey !== publisher.publicKey) {
    die(`${PUBLISHER_KEY} is not the GuusLab publisher key (its public key is ${signed.publisher.publicKey}).`);
  }
  if (canonical(signed.manifest) !== canonical(sub.manifest)) {
    die(`plugins/${id}/plugin.json holds a different manifest than sources/${id}/stride-plugin.json. Make them the same first.`);
  }
  // Keep the submission's own key order: manifest, publisher, release, store.
  // The manifest as the CLI printed it (sorted keys), like every other submission.
  const owner = { id: publisher.id, name: publisher.name, publicKey: publisher.publicKey };
  const next = { manifest: signed.manifest, publisher: owner, release: signed.release, store: sub.store };
  await writeFile(join(root, subPath), JSON.stringify(next, null, 2) + '\n');

  const dest = join(root, 'site', 'plugins', id);
  await mkdir(join(dest, signed.release.version), { recursive: true });
  for (const f of ['plugin.wasm', 'stride-plugin.json']) {
    await copyFile(join(root, 'sources', id, f), join(dest, signed.release.version, f));
  }
  if (sub.store?.icon) await copyFile(join(root, 'plugins', id, sub.store.icon), join(dest, sub.store.icon));
  if (sub.store?.screenshots?.length) {
    await mkdir(join(dest, 'screenshots'), { recursive: true });
    for (const shot of sub.store.screenshots) await copyFile(join(root, 'plugins', id, shot.file), join(dest, shot.file));
  }
  console.log(`signed ${id} ${signed.release.version}  sha256 ${signed.release.contentHash}`);
}

// --- 4. the signed index, the gallery, and the same checks CI runs ----------
run(STRIDE, ['plugin', 'index', '.', '--key', REGISTRY_KEY, '--base-url', BASE], { stdio: 'inherit' });
run('node', ['tools/gallery.mjs'], { stdio: 'inherit' });
run('node', ['tools/verify.mjs', '--no-build'], { stdio: 'inherit' });
console.log('\nDone. Review `git diff --stat`, then commit ("Publish …") and push.');
