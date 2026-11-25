// Fails if package.json, tauri.conf.json and src-tauri/Cargo.toml disagree,
// or (in CI, on a tag) if they disagree with the tag.
import { readFileSync } from 'node:fs';

const pkg = JSON.parse(readFileSync('package.json', 'utf8')).version;
const conf = JSON.parse(readFileSync('src-tauri/tauri.conf.json', 'utf8')).version;
const cargo = readFileSync('src-tauri/Cargo.toml', 'utf8')
  .match(/^version\s*=\s*"([^"]+)"/m)?.[1];

const found = { 'package.json': pkg, 'tauri.conf.json': conf, 'src-tauri/Cargo.toml': cargo };
const unique = [...new Set(Object.values(found))];

if (unique.length !== 1) {
  console.error('Version mismatch:', found);
  process.exit(1);
}

const ref = process.env.GITHUB_REF_NAME;
if (ref?.startsWith('v') && ref.slice(1) !== unique[0]) {
  console.error(`Tag ${ref} does not match version ${unique[0]}`);
  process.exit(1);
}

console.log(`Version ${unique[0]} is consistent.`);
