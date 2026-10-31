#!/usr/bin/env node
// Vendored TaskQ release assets (dashq.md D10, docs/contract/views.md#pinned-taskq-assets).
//   node scripts/taskq-assets.cjs           offline: every pin set names one release
//   node scripts/taskq-assets.cjs --latest  fail when the latest TaskQ release has a newer or missing asset
//   node scripts/taskq-assets.cjs --update  repin every set to the latest release (bytes checked against SHA256SUMS)
'use strict';
const fs = require('fs');
const path = require('path');
const crypto = require('crypto');

const ROOT = path.join(__dirname, '..');
const REPO = 'taskqorg/taskq';
// Two pin-file shapes: {files: {name: sha}, assets: {name: asset}} under assets/taskq/, and {files: {path: {source, sha256}}}.
const SETS = ['assets/taskq/provenance.json', 'assets/taskq/sources.json', 'assets/taskq/workspace-provenance.json'];

const sha = (bytes) => crypto.createHash('sha256').update(bytes).digest('hex');
const read = (p) => JSON.parse(fs.readFileSync(path.join(ROOT, p), 'utf8'));
const prefix = (release) => `taskq-${release.replace(/^v/, '')}-`;

// One row per vendored file: local path, release asset name (null: a repository file such as LICENSE), pinned hash.
function rows(pin) {
  if (pin.assets) return Object.entries(pin.files).map(([name, hash]) => ({local: `assets/taskq/${name}`, key: name, asset: pin.assets[name], sha: hash}));
  return Object.entries(pin.files).map(([local, e]) => ({local, key: local, asset: e.source.startsWith('taskq-') ? e.source : null, file: e.source, sha: e.sha256}));
}

function offline() {
  const bad = [];
  const releases = new Set();
  for (const set of SETS) {
    const pin = read(set);
    releases.add(pin.release);
    for (const r of rows(pin)) {
      if (r.asset && !r.asset.startsWith(prefix(pin.release))) bad.push(`${set}: ${r.key} comes from ${r.asset}, not ${pin.release}`);
    }
  }
  if (releases.size !== 1) bad.push(`pin sets name ${releases.size} releases: ${[...releases].join(', ')}`);
  return {bad, release: [...releases][0]};
}

async function get(url, type) {
  const headers = {'user-agent': 'dashq-taskq-assets'};
  if (process.env.GITHUB_TOKEN && url.startsWith('https://api.github.com/')) headers.authorization = `Bearer ${process.env.GITHUB_TOKEN}`;
  const res = await fetch(url, {headers});
  if (!res.ok) throw new Error(`${url}: HTTP ${res.status}`);
  return type === 'json' ? res.json() : Buffer.from(await res.arrayBuffer());
}

async function latest() {
  const tag = (await get(`https://api.github.com/repos/${REPO}/releases/latest`, 'json')).tag_name;
  const sums = new Map();
  for (const line of (await get(`https://github.com/${REPO}/releases/download/${tag}/SHA256SUMS`)).toString('utf8').split('\n')) {
    const [hash, name] = line.trim().split(/\s+\*?/);
    if (hash && name) sums.set(name, hash);
  }
  return {tag, sums};
}

// Compare each pinned asset with the same asset of the latest release.
function compare(tag, sums) {
  const newer = [];
  for (const set of SETS) {
    const pin = read(set);
    for (const r of rows(pin)) {
      if (!r.asset) continue;
      const name = prefix(tag) + r.asset.slice(prefix(pin.release).length);
      if (!sums.has(name)) newer.push(`${set}: ${r.key}: ${name} is missing from ${tag}`);
      else if (sums.get(name) !== r.sha) newer.push(`${set}: ${r.key}: ${tag} has a newer asset than ${pin.release} (${name})`);
    }
  }
  return newer;
}

async function update(tag, sums) {
  const commit = (await get(`https://api.github.com/repos/${REPO}/commits/${tag}`, 'json')).sha;
  for (const set of SETS) {
    const pin = read(set);
    for (const r of rows(pin)) {
      const name = r.asset ? prefix(tag) + r.asset.slice(prefix(pin.release).length) : r.file;
      const bytes = await get(r.asset ? `https://github.com/${REPO}/releases/download/${tag}/${name}` : `https://raw.githubusercontent.com/${REPO}/${tag}/${name}`);
      const hash = sha(bytes);
      if (r.asset && sums.get(name) !== hash) throw new Error(`${name}: bytes do not match ${tag} SHA256SUMS`);
      fs.writeFileSync(path.join(ROOT, r.local), bytes);
      if (pin.assets) { pin.files[r.key] = hash; pin.assets[r.key] = name; } else pin.files[r.key] = {source: name, sha256: hash};
    }
    pin.release = tag;
    if ('commit' in pin) pin.commit = commit;
    if ('revision' in pin) pin.revision = commit;
    fs.writeFileSync(path.join(ROOT, set), JSON.stringify(pin, null, 2) + '\n');
    console.log(`${set}: ${tag} ${commit}`);
  }
}

async function main() {
  const mode = process.argv[2] || '';
  if (!['', '--latest', '--update'].includes(mode)) throw new Error(`unknown argument ${mode}`);
  if (mode === '--update') { const l = await latest(); await update(l.tag, l.sums); }
  const {bad, release} = offline();
  if (mode === '--latest') { const {tag, sums} = await latest(); bad.push(...compare(tag, sums)); if (!bad.length) console.log(`PASS every pinned asset matches latest TaskQ ${tag}`); }
  if (bad.length) {
    bad.forEach((b) => console.error(`FAIL ${b}`));
    if (mode === '--latest') console.error('Next: node scripts/taskq-assets.cjs --update, then rerun the tests and open a repin PR.');
    process.exit(1);
  }
  console.log(`PASS pin sets name ${release}`);
}

main().catch((e) => { console.error(`Error: ${e.message}`); process.exit(1); });
