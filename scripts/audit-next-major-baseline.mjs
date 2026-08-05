import { promises as fs } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const projectRoot = path.resolve(scriptDir, '..');
const sourceExtensions = new Set(['.ts', '.tsx', '.js', '.jsx']);
const rustExtensions = new Set(['.rs']);

function getSnapshotPath(version) {
  return path.join(
    projectRoot,
    'docs',
    'baselines',
    `pm-center-${version}-interfaces.json`,
  );
}

function normalizePath(filePath) {
  return path.relative(projectRoot, filePath).replaceAll('\\', '/');
}

async function collectFiles(rootPath, extensions) {
  const files = [];
  const stack = [rootPath];

  while (stack.length > 0) {
    const current = stack.pop();
    const entries = await fs.readdir(current, { withFileTypes: true });
    entries.sort((left, right) => left.name.localeCompare(right.name));

    for (const entry of entries) {
      const fullPath = path.join(current, entry.name);
      if (entry.isDirectory()) {
        stack.push(fullPath);
      } else if (entry.isFile() && extensions.has(path.extname(entry.name))) {
        files.push(fullPath);
      }
    }
  }

  return files.sort((left, right) => normalizePath(left).localeCompare(normalizePath(right)));
}

function lineNumberAt(content, index) {
  return content.slice(0, index).split('\n').length;
}

function addReference(target, name, file, line) {
  if (!target.has(name)) {
    target.set(name, []);
  }
  const reference = { file, line };
  const references = target.get(name);
  if (!references.some((item) => item.file === file && item.line === line)) {
    references.push(reference);
  }
}

function mapToSortedEntries(values) {
  return [...values.entries()]
    .sort(([left], [right]) => left.localeCompare(right))
    .map(([name, references]) => ({
      name,
      references: references.sort((left, right) =>
        left.file.localeCompare(right.file) || left.line - right.line),
    }));
}

function extractRegisteredCommands(content) {
  const match = content.match(/\.invoke_handler\s*\(\s*tauri::generate_handler!\[([\s\S]*?)\]\s*\)/m);
  if (!match) {
    throw new Error('Unable to locate tauri::generate_handler! in src-tauri/src/lib.rs');
  }

  return match[1]
    .replaceAll(/\/\/.*$/gm, '')
    .split(',')
    .map((entry) => entry.trim())
    .filter(Boolean)
    .map((entry) => ({
      handler: entry,
      name: entry.split('::').at(-1),
    }))
    .sort((left, right) => left.name.localeCompare(right.name));
}

function extractStringUnion(content, typeName) {
  const expression = new RegExp(`export\\s+type\\s+${typeName}\\s*=([\\s\\S]*?);`, 'm');
  const match = content.match(expression);
  if (!match) {
    return [];
  }
  return [...match[1].matchAll(/['"]([^'"]+)['"]/g)]
    .map((item) => item[1])
    .sort((left, right) => left.localeCompare(right));
}

function extractBuiltinToolIds(content) {
  const start = content.indexOf('export const BUILTIN_TOOLS');
  const end = content.indexOf('export const DEFAULT_PINNED_BUILTIN_TOOL_IDS');
  if (start < 0 || end < 0 || end <= start) {
    return [];
  }
  const section = content.slice(start, end);
  return [...section.matchAll(/\bid:\s*['"]([^'"]+)['"]/g)]
    .map((match) => match[1])
    .sort((left, right) => left.localeCompare(right));
}

function extractRustModules(content) {
  return [...content.matchAll(/^mod\s+([A-Za-z0-9_]+)\s*;/gm)]
    .map((match) => match[1])
    .sort((left, right) => left.localeCompare(right));
}

async function buildSnapshot() {
  const packageJson = JSON.parse(await fs.readFile(path.join(projectRoot, 'package.json'), 'utf8'));
  const libPath = path.join(projectRoot, 'src-tauri', 'src', 'lib.rs');
  const libContent = await fs.readFile(libPath, 'utf8');
  const registeredCommands = extractRegisteredCommands(libContent);
  const registeredNames = new Set(registeredCommands.map((command) => command.name));
  const commandDefinitions = new Map();
  const emittedEventStrings = new Map();
  const frontendInvocations = new Map();
  const frontendEventSubscriptions = new Map();
  const frontendDomEvents = new Map();

  const rustFiles = await collectFiles(path.join(projectRoot, 'src-tauri', 'src'), rustExtensions);
  for (const filePath of rustFiles) {
    const content = await fs.readFile(filePath, 'utf8');
    const relativePath = normalizePath(filePath);

    const commandPattern = /#\s*\[\s*tauri::command(?:\([^\]]*\))?\s*\]\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+([A-Za-z0-9_]+)/g;
    for (const match of content.matchAll(commandPattern)) {
      addReference(commandDefinitions, match[1], relativePath, lineNumberAt(content, match.index));
    }

    for (const match of content.matchAll(/['"](pm-center:[A-Za-z0-9:_-]+)['"]/g)) {
      addReference(emittedEventStrings, match[1], relativePath, lineNumberAt(content, match.index));
    }
  }

  const frontendFiles = await collectFiles(path.join(projectRoot, 'src'), sourceExtensions);
  for (const filePath of frontendFiles) {
    const content = await fs.readFile(filePath, 'utf8');
    const relativePath = normalizePath(filePath);

    const invokePattern = /\binvoke(?:\s*<[^;()]*?>)?\s*\(\s*(['"])([^'"]+)\1/g;
    for (const match of content.matchAll(invokePattern)) {
      addReference(frontendInvocations, match[2], relativePath, lineNumberAt(content, match.index));
    }

    const listenPattern = /(?:\b|\.)(?:listen|once)(?:\s*<[^;()]*?>)?\s*\(\s*(['"])([^'"]+)\1/g;
    for (const match of content.matchAll(listenPattern)) {
      addReference(
        frontendEventSubscriptions,
        match[2],
        relativePath,
        lineNumberAt(content, match.index),
      );
    }

    const domEventPattern = /(?:addEventListener\s*\(|new\s+(?:Custom)?Event\s*\()\s*(['"])([^'"]+)\1/g;
    for (const match of content.matchAll(domEventPattern)) {
      addReference(frontendDomEvents, match[2], relativePath, lineNumberAt(content, match.index));
    }
  }

  const definitionNames = new Set(commandDefinitions.keys());
  const invocationNames = new Set(frontendInvocations.keys());
  const workspaceTabContent = await fs.readFile(
    path.join(projectRoot, 'src', 'stores', 'workspaceTabStore.ts'),
    'utf8',
  );
  const shellTabContent = await fs.readFile(
    path.join(projectRoot, 'src', 'stores', 'shellTabStore.ts'),
    'utf8',
  );
  const builtinToolsContent = await fs.readFile(
    path.join(projectRoot, 'src', 'features', 'builtinTools.ts'),
    'utf8',
  );

  return {
    schemaVersion: 1,
    sourceVersion: packageJson.version,
    generator: 'scripts/audit-next-major-baseline.mjs',
    limitations: [
      'Frontend invoke/listen references are collected only when the command or event name is a direct string literal.',
      'Rust event inventory collects pm-center:* string literals and does not prove that every string is emitted at runtime.',
      'Registered commands without a literal frontend invocation may be compatibility APIs or targets reached through dynamic wrappers; they are not automatically dead code.',
    ],
    rustModules: extractRustModules(libContent),
    tauriCommands: {
      registered: registeredCommands,
      definitions: mapToSortedEntries(commandDefinitions),
      registeredWithoutDefinition: [...registeredNames]
        .filter((name) => !definitionNames.has(name))
        .sort((left, right) => left.localeCompare(right)),
      definitionsNotRegistered: [...definitionNames]
        .filter((name) => !registeredNames.has(name))
        .sort((left, right) => left.localeCompare(right)),
      frontendInvocations: mapToSortedEntries(frontendInvocations),
      frontendInvocationsNotRegistered: [...invocationNames]
        .filter((name) => !registeredNames.has(name))
        .sort((left, right) => left.localeCompare(right)),
      registeredWithoutFrontendInvocation: [...registeredNames]
        .filter((name) => !invocationNames.has(name))
        .sort((left, right) => left.localeCompare(right)),
    },
    events: {
      pmCenterStringsInRust: mapToSortedEntries(emittedEventStrings),
      frontendSubscriptions: mapToSortedEntries(frontendEventSubscriptions),
      frontendDomEvents: mapToSortedEntries(frontendDomEvents),
    },
    frontendRegistries: {
      builtinToolIds: extractBuiltinToolIds(builtinToolsContent),
      workspaceTabTypes: extractStringUnion(workspaceTabContent, 'WorkspaceTabType'),
      shellTabTypes: extractStringUnion(shellTabContent, 'ShellTabType'),
    },
  };
}

function summary(snapshot) {
  return [
    `Nexora ${snapshot.sourceVersion} interface baseline`,
    `Rust modules: ${snapshot.rustModules.length}`,
    `Registered Tauri commands: ${snapshot.tauriCommands.registered.length}`,
    `Tauri command definitions: ${snapshot.tauriCommands.definitions.length}`,
    `Frontend invoke targets: ${snapshot.tauriCommands.frontendInvocations.length}`,
    `Rust pm-center event strings: ${snapshot.events.pmCenterStringsInRust.length}`,
    `Frontend event subscriptions: ${snapshot.events.frontendSubscriptions.length}`,
    `Builtin tools: ${snapshot.frontendRegistries.builtinToolIds.length}`,
    `Workspace tab types: ${snapshot.frontendRegistries.workspaceTabTypes.length}`,
    `Shell tab types: ${snapshot.frontendRegistries.shellTabTypes.length}`,
  ].join('\n');
}

async function main() {
  const mode = process.argv[2] ?? '--summary';
  const snapshot = await buildSnapshot();
  const snapshotPath = getSnapshotPath(snapshot.sourceVersion);
  const serialized = `${JSON.stringify(snapshot, null, 2)}\n`;

  if (mode === '--write') {
    await fs.mkdir(path.dirname(snapshotPath), { recursive: true });
    await fs.writeFile(snapshotPath, serialized, 'utf8');
    console.log(`${summary(snapshot)}\nSnapshot written: ${normalizePath(snapshotPath)}`);
    return;
  }

  if (mode === '--check') {
    let current;
    try {
      current = await fs.readFile(snapshotPath, 'utf8');
    } catch {
      throw new Error(`Baseline snapshot is missing: ${normalizePath(snapshotPath)}`);
    }
    if (current !== serialized) {
      throw new Error(
        `Interface baseline changed. Review it, then run npm run audit:baseline:write to accept the new snapshot.`,
      );
    }
    console.log(`${summary(snapshot)}\nBaseline snapshot matches source.`);
    return;
  }

  if (mode === '--json') {
    process.stdout.write(serialized);
    return;
  }

  if (mode !== '--summary') {
    throw new Error(`Unknown mode: ${mode}`);
  }
  console.log(summary(snapshot));
}

main().catch((error) => {
  console.error(`[baseline-audit] ${error.message}`);
  process.exitCode = 1;
});
