import { mkdir, copyFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { resolve } from 'node:path';
import { spawn } from 'node:child_process';

const root = process.cwd();
const manifest = resolve(root, 'src-tauri', 'Cargo.toml');
const profile = process.argv.includes('--debug') || process.env.NODE_ENV === 'development' ? 'debug' : 'release';
const executableName = process.platform === 'win32' ? 'pmc-component-host.exe' : 'pmc-component-host';
const target = resolve(root, 'src-tauri', 'target', profile, executableName);
const resourceDir = resolve(root, 'src-tauri', 'resources', 'component-host');
const destination = resolve(resourceDir, executableName);

const cargoArgs = ['build', '--manifest-path', manifest, '--bin', 'pmc-component-host'];
if (profile === 'release') cargoArgs.push('--release');

await new Promise((resolvePromise, reject) => {
  const child = spawn('cargo', cargoArgs, { cwd: root, stdio: 'inherit', shell: process.platform === 'win32' });
  child.once('error', reject);
  child.once('exit', (code) => code === 0 ? resolvePromise() : reject(new Error('cargo build pmc-component-host exited with ' + code)));
});

if (!existsSync(target)) throw new Error('未找到宿主产物: ' + target);
await mkdir(resourceDir, { recursive: true });
await copyFile(target, destination);
console.log('[component-host] prepared ' + destination);
