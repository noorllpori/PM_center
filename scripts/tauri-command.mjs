import { spawn } from 'node:child_process';
import { syncAppVersionFiles, ensureVersionedExeArtifacts } from './app-version.mjs';

const args = process.argv.slice(2);
const isDebugBuild = args.includes('--debug');

await syncAppVersionFiles();

const command = process.platform === 'win32' ? 'cmd.exe' : 'npx';
const commandArgs = process.platform === 'win32'
  ? ['/d', '/s', '/c', 'npx', 'tauri', ...args]
  : ['tauri', ...args];

const child = spawn(command, commandArgs, {
  stdio: 'inherit',
  cwd: process.cwd(),
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
