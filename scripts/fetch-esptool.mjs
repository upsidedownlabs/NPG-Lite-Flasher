// Downloads the esptool build Arduino IDE uses (ESP32 core 3.3.12 ships esptool 5.3.1)
// and places it where Tauri expects a sidecar: src-tauri/binaries/npg-esptool-<target-triple>[.exe]
import { execSync } from 'node:child_process';
import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';

const VERSION = 'v5.3.1';
const BUILDS = {
  'x86_64-pc-windows-msvc': { name: 'windows-amd64', ext: 'zip', sha256: '2b4a73c45db27426685896f64ce3e557f63a64f43cc100cb65c0cc3486af96d3' },
  'x86_64-unknown-linux-gnu': { name: 'linux-amd64', ext: 'tar.gz', sha256: 'e9cc641f8e4a0b644b52836d7a6b59f3c6d3261213c5ccc41f8f3c3035d06aa4' },
  'aarch64-apple-darwin': { name: 'macos-arm64', ext: 'tar.gz', sha256: 'f63f7203d88cfe4c17aea34d6cf82769458ce204e49a05816c6384c2d299e6ca' },
};

const triple = execSync('rustc -vV').toString().match(/host: (\S+)/)[1];
const build = BUILDS[triple];
if (!build) throw new Error(`No esptool build for ${triple}`);

const url = `https://github.com/espressif/esptool/releases/download/${VERSION}/esptool-${VERSION}-${build.name}.${build.ext}`;
const dir = path.resolve('src-tauri/binaries');
const tmp = path.join(dir, 'tmp');
fs.rmSync(tmp, { recursive: true, force: true });
fs.mkdirSync(tmp, { recursive: true });

const res = await fetch(url);
if (!res.ok) throw new Error(`Download failed (${res.status}): ${url}`);
const data = Buffer.from(await res.arrayBuffer());
const hash = crypto.createHash('sha256').update(data).digest('hex');
if (hash !== build.sha256) throw new Error(`Checksum mismatch for ${url}: ${hash}`);

const archive = path.join(tmp, `esptool.${build.ext}`);
fs.writeFileSync(archive, data);
execSync(`tar -xf "${archive}" -C "${tmp}"`);

const exe = triple.includes('windows') ? '.exe' : '';
const dest = path.join(dir, `npg-esptool-${triple}${exe}`);
fs.copyFileSync(path.join(tmp, `esptool-${build.name}`, `esptool${exe}`), dest);
fs.chmodSync(dest, 0o755);
fs.rmSync(tmp, { recursive: true, force: true });
console.log(`esptool ${VERSION} ready: ${dest}`);
