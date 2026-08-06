import { copyFile, mkdir } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { resolve } from 'node:path';
import { spawn } from 'node:child_process';

const root = process.cwd();
const manifest = resolve(root, 'src-tauri', 'Cargo.toml');
const profile = process.argv.includes('--debug') || process.env.NODE_ENV === 'development' ? 'debug' : 'release';
const executableName = process.platform === 'win32' ? 'pmc-blendio-service.exe' : 'pmc-blendio-service';
const source = resolve(root, 'src-tauri', 'target', profile, executableName);
const resourceDirectory = resolve(root, 'src-tauri', 'resources', 'blendio-service');
const destination = resolve(resourceDirectory, executableName);

const cargoArgs = ['build', '--manifest-path', manifest, '--bin', 'pmc-blendio-service'];
if (profile === 'release') cargoArgs.push('--release');
await new Promise((resolvePromise, reject) => {
  const child = spawn('cargo', cargoArgs, { cwd: root, stdio: 'inherit', shell: process.platform === 'win32' });
  child.once('error', reject);
  child.once('exit', (code) => code === 0 ? resolvePromise() : reject(new Error('cargo build pmc-blendio-service exited with ' + code)));
});
if (!existsSync(source)) throw new Error('未找到 BlendIO 服务产物: ' + source);
await mkdir(resourceDirectory, { recursive: true });
await copyFile(source, destination);
console.log('[blendio-service] prepared ' + destination);
