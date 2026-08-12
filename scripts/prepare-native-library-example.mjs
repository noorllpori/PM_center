import { copyFile, mkdir } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { resolve } from 'node:path';
import { spawn } from 'node:child_process';

const root = process.cwd();
const profile = process.argv.includes('--debug') || process.env.NODE_ENV === 'development' ? 'debug' : 'release';
const exampleRoot = resolve(root, 'examples', 'runtime', 'native-library-echo');
const manifest = resolve(exampleRoot, 'Cargo.toml');
const targetDirectory = resolve(root, 'target', 'native-library-echo');
const libraryName = process.platform === 'win32'
  ? 'nexora_native_library_echo.dll'
  : process.platform === 'darwin'
    ? 'libnexora_native_library_echo.dylib'
    : 'libnexora_native_library_echo.so';
const source = resolve(targetDirectory, profile, libraryName);
const destinationDirectory = resolve(exampleRoot, 'bin', 'windows-x64');
const destination = resolve(destinationDirectory, libraryName);

if (process.platform !== 'win32') {
  console.log('[native-library-example] skipped outside Windows');
  process.exit(0);
}

const cargoArgs = ['build', '--manifest-path', manifest, '--target-dir', targetDirectory];
if (profile === 'release') cargoArgs.push('--release');
await new Promise((resolvePromise, reject) => {
  const child = spawn('cargo', cargoArgs, { cwd: root, stdio: 'inherit', shell: process.platform === 'win32' });
  child.once('error', reject);
  child.once('exit', (code) => code === 0 ? resolvePromise() : reject(new Error('cargo build native-library example exited with ' + code)));
});

if (!existsSync(source)) throw new Error('未找到 native-library 示例产物: ' + source);
await mkdir(destinationDirectory, { recursive: true });
await copyFile(source, destination);
console.log('[native-library-example] prepared ' + destination);
