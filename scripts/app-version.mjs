import { promises as fs } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const projectRoot = path.resolve(scriptDir, '..');

const tauriConfigPath = path.join(projectRoot, 'src-tauri', 'tauri.conf.json');
const packageJsonPath = path.join(projectRoot, 'package.json');
const packageLockPath = path.join(projectRoot, 'package-lock.json');
const cargoTomlPath = path.join(projectRoot, 'src-tauri', 'Cargo.toml');
const targetRootDir = path.join(projectRoot, 'src-tauri', 'target');

function sanitizeFileName(input) {
  return input.replace(/[<>:"/\\|?*\u0000-\u001f]/g, ' ').replace(/\s+/g, ' ').trim();
}

async function pathExists(targetPath) {
  try {
    await fs.access(targetPath);
    return true;
  } catch {
    return false;
  }
}

async function readJson(filePath) {
  return JSON.parse(await fs.readFile(filePath, 'utf8'));
}

async function writeJson(filePath, value) {
  await fs.writeFile(filePath, `${JSON.stringify(value, null, 2)}\r\n`, 'utf8');
}

function extractCargoPackageName(cargoToml) {
  const match = cargoToml.match(/\[package\][\s\S]*?^\s*name\s*=\s*"([^"]+)"/m);
  if (!match) {
    throw new Error('Failed to locate Cargo package name in src-tauri/Cargo.toml');
  }
  return match[1];
}

function syncCargoVersion(cargoToml, version) {
  const versionPattern = /(\[package\][\s\S]*?^\s*version\s*=\s*")([^"]+)(")/m;
  const matched = cargoToml.match(versionPattern);

  if (!matched) {
    throw new Error('Failed to locate Cargo package version in src-tauri/Cargo.toml');
  }

  if (matched[2] === version) {
    return cargoToml;
  }

  return cargoToml.replace(versionPattern, `$1${version}$3`);
}

async function collectExeFiles(rootPath) {
  if (!(await pathExists(rootPath))) {
    return [];
  }

  const result = [];
  const stack = [rootPath];
  const ignoredDirectories = new Set(['deps', '.fingerprint', 'incremental']);

  while (stack.length > 0) {
    const current = stack.pop();
    const entries = await fs.readdir(current, { withFileTypes: true });

    for (const entry of entries) {
      const fullPath = path.join(current, entry.name);
      if (entry.isDirectory()) {
        if (ignoredDirectories.has(entry.name)) {
          continue;
        }
        stack.push(fullPath);
        continue;
      }

      if (entry.isFile() && entry.name.toLowerCase().endsWith('.exe')) {
        result.push(fullPath);
      }
    }
  }

  return result;
}

function shouldVersionExe(filePath, metadata) {
  const baseName = path.basename(filePath).toLowerCase();
  const versionToken = metadata.version.toLowerCase();
  const genericVersionPattern = /(^|[_ -])\d+\.\d+\.\d+([_ -]|$)/;

  if (baseName.includes(versionToken)) {
    return false;
  }

  if (genericVersionPattern.test(baseName)) {
    return false;
  }

  const cargoBinaryName = `${metadata.cargoPackageName}.exe`.toLowerCase();
  const cargoBinaryDashName = `${metadata.cargoPackageName.replace(/_/g, '-')}.exe`.toLowerCase();
  const productExeName = `${metadata.productName}.exe`.toLowerCase();

  if (baseName === cargoBinaryName || baseName === cargoBinaryDashName || baseName === productExeName) {
    return true;
  }

  const bundleSegment = `${path.sep}bundle${path.sep}`.toLowerCase();
  if (filePath.toLowerCase().includes(bundleSegment) && baseName.startsWith(metadata.productName.toLowerCase())) {
    return true;
  }

  return false;
}

function buildVersionedExeName(filePath, metadata) {
  const originalName = path.basename(filePath, '.exe');
  const cargoBase = metadata.cargoPackageName.toLowerCase();
  const normalizedOriginal = originalName.toLowerCase();
  const preferredBaseName =
    normalizedOriginal === cargoBase || normalizedOriginal === cargoBase.replace(/_/g, '-')
      ? sanitizeFileName(metadata.productName)
      : originalName;

  return `${preferredBaseName}_${metadata.version}.exe`;
}

export async function loadAppVersionMetadata() {
  const [tauriConfig, cargoToml] = await Promise.all([
    readJson(tauriConfigPath),
    fs.readFile(cargoTomlPath, 'utf8'),
  ]);

  return {
    version: tauriConfig.version,
    productName: tauriConfig.productName,
    identifier: tauriConfig.identifier,
    cargoPackageName: extractCargoPackageName(cargoToml),
  };
}

export async function syncAppVersionFiles({ log = true } = {}) {
  const metadata = await loadAppVersionMetadata();
  const changedFiles = [];

  const packageJson = await readJson(packageJsonPath);
  if (packageJson.version !== metadata.version) {
    packageJson.version = metadata.version;
    await writeJson(packageJsonPath, packageJson);
    changedFiles.push('package.json');
  }

  if (await pathExists(packageLockPath)) {
    const packageLock = await readJson(packageLockPath);
    const rootPackage = packageLock.packages?.[''];
    let packageLockChanged = false;

    if (packageLock.version !== metadata.version) {
      packageLock.version = metadata.version;
      packageLockChanged = true;
    }

    if (rootPackage && rootPackage.version !== metadata.version) {
      rootPackage.version = metadata.version;
      packageLockChanged = true;
    }

    if (packageLockChanged) {
      await writeJson(packageLockPath, packageLock);
      changedFiles.push('package-lock.json');
    }
  }

  const cargoToml = await fs.readFile(cargoTomlPath, 'utf8');
  const nextCargoToml = syncCargoVersion(cargoToml, metadata.version);
  if (nextCargoToml !== cargoToml) {
    await fs.writeFile(cargoTomlPath, nextCargoToml, 'utf8');
    changedFiles.push('src-tauri/Cargo.toml');
  }

  if (log) {
    if (changedFiles.length > 0) {
      console.log(`[version] synced ${metadata.version} -> ${changedFiles.join(', ')}`);
    } else {
      console.log(`[version] already synced (${metadata.version})`);
    }
  }

  return metadata;
}

export async function ensureVersionedExeArtifacts({ log = true, targetDirName = 'release' } = {}) {
  const metadata = await loadAppVersionMetadata();
  const exeFiles = await collectExeFiles(path.join(targetRootDir, targetDirName));
  const createdFiles = [];

  for (const exeFile of exeFiles) {
    if (!shouldVersionExe(exeFile, metadata)) {
      continue;
    }

    const nextName = buildVersionedExeName(exeFile, metadata);
    const nextPath = path.join(path.dirname(exeFile), nextName);

    if (path.resolve(nextPath) === path.resolve(exeFile)) {
      continue;
    }

    await fs.copyFile(exeFile, nextPath);
    createdFiles.push(path.relative(projectRoot, nextPath));
  }

  if (log) {
    if (createdFiles.length > 0) {
      console.log(`[version] created versioned executables:\n${createdFiles.map(file => `  - ${file}`).join('\n')}`);
    } else {
      console.log('[version] no extra executable renaming was needed');
    }
  }

  return createdFiles;
}
