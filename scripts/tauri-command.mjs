import { spawn } from 'node:child_process';
import { access } from 'node:fs/promises';
import { constants } from 'node:fs';
import { resolve } from 'node:path';
import { syncAppVersionFiles, ensureVersionedExeArtifacts } from './app-version.mjs';

const args = process.argv.slice(2);
const isDebugBuild = args.includes('--debug');
const isDevCommand = args[0] === 'dev';

await syncAppVersionFiles();

if (isDevCommand && !process.env.PMC_ALLOW_PLUGIN_SYSTEM_PYTHON) {
  process.env.PMC_ALLOW_PLUGIN_SYSTEM_PYTHON = '1';
  console.log(
    '[tauri-wrapper] dev mode allows plugin system Python fallback when embedded runtime is unavailable.',
  );
}

const tauriCliEntry = resolve(
  process.cwd(),
  'node_modules',
  '@tauri-apps',
  'cli',
  'tauri.js',
);

try {
  await access(tauriCliEntry, constants.F_OK);
} catch {
  console.error(
    '[tauri-wrapper] missing local @tauri-apps/cli. Run "npm install" in the project root before starting Tauri.',
  );
  process.exit(1);
}

const child = spawn(process.execPath, [tauriCliEntry, ...args], {
  stdio: 'inherit',
  cwd: process.cwd(),
  env: process.env,
});

child.on('error', (error) => {
  console.error('[tauri-wrapper] failed to start tauri cli:', error);
  process.exit(1);
});

child.on('close', async (code) => {
  if (code === 0 && args[0] === 'build') {
    try {
      await ensureVersionedExeArtifacts({
        targetDirName: isDebugBuild ? 'debug' : 'release',
      });
    } catch (error) {
      console.error('[tauri-wrapper] build finished but versioned exe generation failed:', error);
      process.exit(1);
      return;
    }
  }

  process.exit(code ?? 0);
});
