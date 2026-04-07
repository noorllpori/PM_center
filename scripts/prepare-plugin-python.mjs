import { spawnSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

const VERSION = process.env.PMC_PLUGIN_PYTHON_VERSION || '3.11.9';
const IS_WINDOWS = process.platform === 'win32';
const SKIP = process.env.PMC_SKIP_PLUGIN_PYTHON_DOWNLOAD === '1';
const OPTIONAL = process.argv.includes('--optional') || process.env.PMC_PLUGIN_PYTHON_OPTIONAL === '1';
const PLUGIN_PYTHON_ROOT = resolve(process.cwd(), 'src-tauri', 'resources', 'plugin-python');
const RUNTIME_DIR = join(PLUGIN_PYTHON_ROOT, 'windows-x64');
const ARCHIVE_PATH = join(PLUGIN_PYTHON_ROOT, `python-${VERSION}-embed-amd64.zip`);
const GET_PIP_PATH = join(PLUGIN_PYTHON_ROOT, 'get-pip.py');
const MARKER_PATH = join(RUNTIME_DIR, '.pmc-runtime-version');
const DEFAULT_PYTHON_URL = `https://www.python.org/ftp/python/${VERSION}/python-${VERSION}-embed-amd64.zip`;
const DEFAULT_GET_PIP_URL = 'https://bootstrap.pypa.io/get-pip.py';
const DOWNLOAD_TIMEOUT_MS = parsePositiveInteger(process.env.PMC_PLUGIN_PYTHON_TIMEOUT_MS, 120_000);
const DOWNLOAD_RETRIES = parsePositiveInteger(process.env.PMC_PLUGIN_PYTHON_DOWNLOAD_ATTEMPTS, 2);
const DOWNLOAD_TIMEOUT_SECONDS = Math.max(1, Math.ceil(DOWNLOAD_TIMEOUT_MS / 1000));
const PYTHON_URLS = [
  process.env.PMC_PLUGIN_PYTHON_URL,
  ...splitList(process.env.PMC_PLUGIN_PYTHON_FALLBACK_URLS),
  DEFAULT_PYTHON_URL,
].filter((value, index, list) => Boolean(value) && list.indexOf(value) === index);
const GET_PIP_URLS = [
  process.env.PMC_PLUGIN_GET_PIP_URL,
  DEFAULT_GET_PIP_URL,
].filter((value, index, list) => Boolean(value) && list.indexOf(value) === index);

function parsePositiveInteger(value, fallback) {
  const parsed = Number.parseInt(value ?? '', 10);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : fallback;
}

function splitList(value) {
  return (value ?? '')
    .split(',')
    .map((item) => item.trim())
    .filter(Boolean);
}

function quoteForPowerShell(value) {
  return `'${value.replace(/'/g, "''")}'`;
}

function formatError(error) {
  if (error instanceof Error) {
    if (error.cause instanceof Error) {
      return `${error.message} (${error.cause.message})`;
    }
    return error.message;
  }
  return String(error);
}

async function ensureDirectory(targetPath) {
  await mkdir(targetPath, { recursive: true });
}

async function downloadFileWithFetch(url, outputPath) {
  const response = await fetch(url, {
    signal: AbortSignal.timeout(DOWNLOAD_TIMEOUT_MS),
  });
  if (!response.ok) {
    throw new Error(`download failed: ${response.status} ${response.statusText}`);
  }

  const arrayBuffer = await response.arrayBuffer();
  await ensureDirectory(dirname(outputPath));
  await writeFile(outputPath, Buffer.from(arrayBuffer));
}

function downloadFileWithPowerShell(url, outputPath) {
  const command = [
    '-NoProfile',
    '-Command',
    [
      '$ProgressPreference = "SilentlyContinue"',
      `Invoke-WebRequest -Uri ${quoteForPowerShell(url)} -OutFile ${quoteForPowerShell(outputPath)} -UseBasicParsing -TimeoutSec ${DOWNLOAD_TIMEOUT_SECONDS}`,
    ].join('; '),
  ];
  const result = spawnSync('powershell', command, {
    stdio: 'inherit',
    cwd: process.cwd(),
  });

  if (result.status !== 0) {
    throw new Error(`PowerShell download failed with exit code ${result.status ?? 'unknown'}`);
  }
}

async function downloadFile(urls, outputPath, label) {
  const errors = [];

  for (const url of urls) {
    for (let attempt = 1; attempt <= DOWNLOAD_RETRIES; attempt += 1) {
      await rm(outputPath, { force: true }).catch(() => {});
      console.log(`[plugin-python] downloading ${label} from ${url} (attempt ${attempt}/${DOWNLOAD_RETRIES})`);

      try {
        await downloadFileWithFetch(url, outputPath);
        return;
      } catch (error) {
        const message = formatError(error);
        errors.push(`${url} via fetch: ${message}`);
        console.warn(`[plugin-python] fetch failed: ${message}`);

        if (IS_WINDOWS) {
          try {
            console.log(`[plugin-python] retrying ${label} download with PowerShell for ${url}`);
            downloadFileWithPowerShell(url, outputPath);
            return;
          } catch (powershellError) {
            const powershellMessage = formatError(powershellError);
            errors.push(`${url} via powershell: ${powershellMessage}`);
            console.warn(`[plugin-python] PowerShell download failed: ${powershellMessage}`);
          }
        }

        if (attempt < DOWNLOAD_RETRIES) {
          await delay(Math.min(5_000, attempt * 1_000));
        }
      }
    }
  }

  throw new Error(`unable to download ${label}. Tried: ${errors.join(' | ')}`);
}

async function extractArchive() {
  await rm(RUNTIME_DIR, { recursive: true, force: true });
  await ensureDirectory(RUNTIME_DIR);

  const command = [
    '-NoProfile',
    '-Command',
    `Expand-Archive -LiteralPath ${quoteForPowerShell(ARCHIVE_PATH)} -DestinationPath ${quoteForPowerShell(RUNTIME_DIR)} -Force`,
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

async function ensureGetPipScript() {
  if (existsSync(GET_PIP_PATH)) {
    console.log(`[plugin-python] using cached get-pip bootstrap ${GET_PIP_PATH}`);
    return;
  }

  await downloadFile(GET_PIP_URLS, GET_PIP_PATH, 'get-pip.py');
}

function hasSystemPython() {
  const result = spawnSync('python', ['--version'], {
    stdio: 'ignore',
    cwd: process.cwd(),
  });
  return result.status === 0;
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
    await ensureGetPipScript();
    return;
  }

  console.log(`[plugin-python] preparing embedded runtime ${VERSION}`);

  try {
    if (!existsSync(ARCHIVE_PATH)) {
      await downloadFile(PYTHON_URLS, ARCHIVE_PATH, 'embedded runtime');
    } else {
      console.log(`[plugin-python] using cached archive ${ARCHIVE_PATH}`);
    }

    await extractArchive();
    await patchRuntime();
    await ensureGetPipScript();
    console.log(`[plugin-python] ready at ${RUNTIME_DIR}`);
  } catch (error) {
    if (OPTIONAL && hasSystemPython()) {
      console.warn(`[plugin-python] continuing without embedded runtime: ${formatError(error)}`);
      console.warn('[plugin-python] plugin dependency management will stay unavailable until the embedded runtime is prepared.');
      return;
    }

    throw error;
  }
}

main().catch((error) => {
  console.error('[plugin-python] failed:', error);
  process.exit(1);
});
