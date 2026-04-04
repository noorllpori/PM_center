import { spawnSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';

const VERSION = process.env.PMC_PLUGIN_PYTHON_VERSION || '3.11.9';
const IS_WINDOWS = process.platform === 'win32';
const SKIP = process.env.PMC_SKIP_PLUGIN_PYTHON_DOWNLOAD === '1';
const RUNTIME_DIR = resolve(process.cwd(), 'src-tauri', 'resources', 'plugin-python', 'windows-x64');
const ARCHIVE_PATH = resolve(process.cwd(), 'src-tauri', 'resources', 'plugin-python', `python-${VERSION}-embed-amd64.zip`);
const MARKER_PATH = join(RUNTIME_DIR, '.pmc-runtime-version');
const PYTHON_URL = `https://www.python.org/ftp/python/${VERSION}/python-${VERSION}-embed-amd64.zip`;

async function ensureDirectory(targetPath) {
  await mkdir(targetPath, { recursive: true });
}

async function downloadArchive() {
  const response = await fetch(PYTHON_URL);
  if (!response.ok) {
    throw new Error(`download failed: ${response.status} ${response.statusText}`);
  }

  const arrayBuffer = await response.arrayBuffer();
  await ensureDirectory(dirname(ARCHIVE_PATH));
  await writeFile(ARCHIVE_PATH, Buffer.from(arrayBuffer));
}

async function extractArchive() {
  await rm(RUNTIME_DIR, { recursive: true, force: true });
  await ensureDirectory(RUNTIME_DIR);

  const command = [
    '-NoProfile',
    '-Command',
    `Expand-Archive -LiteralPath '${ARCHIVE_PATH.replace(/'/g, "''")}' -DestinationPath '${RUNTIME_DIR.replace(/'/g, "''")}' -Force`,
  ];
  const result = spawnSync('powershell', command, {
    stdio: 'inherit',
    cwd: process.cwd(),
  });

  if (result.status !== 0) {
    throw new Error('failed to extract embedded python archive');
  }
}

async function patchRuntime() {
  const pthCandidates = ['python311._pth', 'python._pth'];
  for (const filename of pthCandidates) {
    const target = join(RUNTIME_DIR, filename);
    if (existsSync(target)) {
      await rm(target, { force: true });
    }
  }

  await writeFile(MARKER_PATH, `${VERSION}\n`);
}

async function isPrepared() {
  const pythonExe = join(RUNTIME_DIR, 'python.exe');
  if (!existsSync(pythonExe) || !existsSync(MARKER_PATH)) {
    return false;
  }

  const marker = await readFile(MARKER_PATH, 'utf8').catch(() => '');
  return marker.trim() === VERSION;
}

async function main() {
  if (!IS_WINDOWS) {
    console.log('[plugin-python] skip: embedded runtime is only prepared automatically on Windows.');
    return;
  }

  if (SKIP) {
    console.log('[plugin-python] skip: PMC_SKIP_PLUGIN_PYTHON_DOWNLOAD=1');
    return;
  }

  if (await isPrepared()) {
    console.log(`[plugin-python] runtime already prepared (${VERSION})`);
    return;
  }

  console.log(`[plugin-python] preparing embedded runtime ${VERSION}`);

  if (!existsSync(ARCHIVE_PATH)) {
    console.log(`[plugin-python] downloading ${PYTHON_URL}`);
    await downloadArchive();
  } else {
    console.log(`[plugin-python] using cached archive ${ARCHIVE_PATH}`);
  }

  await extractArchive();
  await patchRuntime();
  console.log(`[plugin-python] ready at ${RUNTIME_DIR}`);
}

main().catch((error) => {
  console.error('[plugin-python] failed:', error);
  process.exit(1);
});
