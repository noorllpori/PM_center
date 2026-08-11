import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import Ajv from 'ajv';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const contractRoot = path.join(root, 'shared', 'pmc-platform');
const schemaRoot = path.join(contractRoot, 'schemas', 'v1');
const fixtureRoot = path.join(contractRoot, 'fixtures', 'v1');

const cases = [
  ['module-manifest.schema.json', 'module-manifest.json'],
  ['component-manifest.schema.json', 'component-manifest.json'],
  ['workspace-profile.schema.json', 'workspace-profile.json'],
  ['presentation-template.schema.json', 'presentation-template.json'],
  ['workflow-manifest.schema.json', 'workflow-manifest.json'],
  ['package-header.schema.json', 'package-header.json'],
  ['contract-error.schema.json', 'contract-error.json'],
];

const ajv = new Ajv({ allErrors: true, strict: false });
for (const format of ['uint8', 'uint16', 'uint64', 'int32']) {
  ajv.addFormat(format, true);
}
const validators = new Map();
for (const [schemaName, fixtureName] of cases) {
  const schema = readJson(path.join(schemaRoot, schemaName));
  const fixture = readJson(path.join(fixtureRoot, 'valid', fixtureName));
  const validate = ajv.compile(schema);
  validators.set(schemaName, validate);
  if (!validate(fixture)) {
    throw new Error(
      `${fixtureName} does not match ${schemaName}:\n${JSON.stringify(validate.errors, null, 2)}`,
    );
  }
}

const componentExamplePaths = [
  'examples/script-project-storage/component.json',
  'examples/script-automation-blend-audit/component.json',
  'examples/ninniku-music-player/component.json',
  'examples/blank-home-template/component.json',
  'examples/split-interface-template/component.json',
];
const validateComponentExample = validators.get('component-manifest.schema.json');
for (const relativePath of componentExamplePaths) {
  const example = readJson(path.join(root, relativePath));
  if (!validateComponentExample(example)) {
    throw new Error(
      `${relativePath} does not match component-manifest.schema.json:\n${JSON.stringify(validateComponentExample.errors, null, 2)}`,
    );
  }
}

const presentationExamplePaths = [
  'examples/blank-home-template/presentation/shell/blank-home/template.json',
  'examples/split-interface-template/presentation/shell/split-50/template.json',
];
const validatePresentationExample = validators.get('presentation-template.schema.json');
for (const relativePath of presentationExamplePaths) {
  const example = readJson(path.join(root, relativePath));
  if (!validatePresentationExample(example)) {
    throw new Error(
      `${relativePath} does not match presentation-template.schema.json:\n${JSON.stringify(validatePresentationExample.errors, null, 2)}`,
    );
  }
}

const unknownCapability = readJson(
  path.join(fixtureRoot, 'invalid', 'module-unknown-capability.json'),
);
if (validators.get('module-manifest.schema.json')(unknownCapability)) {
  throw new Error('JSON Schema accepted an unknown capability');
}

const futureProfile = readJson(
  path.join(fixtureRoot, 'invalid', 'profile-future-schema.json'),
);
if (validators.get('workspace-profile.schema.json')(futureProfile)) {
  throw new Error('JSON Schema accepted an unsupported schemaVersion');
}

const capabilitySchema = readJson(path.join(schemaRoot, 'component-manifest.schema.json'));
const schemaCapabilities = findEnumContaining(capabilitySchema, 'project.storage.read');
if (!schemaCapabilities) {
  throw new Error('Component schema does not expose the Capability enum');
}
const capabilitySource = fs.readFileSync(path.join(contractRoot, 'src', 'capability.rs'), 'utf8');
const rustCapabilities = [...capabilitySource.matchAll(/#\[serde\(rename = "([a-z0-9.-]+)"\)\]/g)]
  .map((match) => match[1]);
const typescriptSource = fs.readFileSync(path.join(root, 'src', 'types', 'platform.ts'), 'utf8');
const capabilityBlock = typescriptSource.match(/export type Capability =([\s\S]*?);/);
const typescriptCapabilities = capabilityBlock
  ? [...capabilityBlock[1].matchAll(/'([a-z0-9.-]+)'/g)].map((match) => match[1])
  : [];
assertSameSet('Rust Capability', rustCapabilities, 'schema Capability', schemaCapabilities);
assertSameSet('TypeScript Capability', typescriptCapabilities, 'schema Capability', schemaCapabilities);

const sdkPath = path.join(root, 'src-tauri', 'resources', 'script-sdk', 'nexora_sdk', '__init__.py');
const sdkSource = fs.readFileSync(sdkPath, 'utf8');
const interfaceReference = fs.readFileSync(
  path.join(root, 'docs', 'extension-development', 'INTERFACE_REFERENCE.md'),
  'utf8',
);
const aiReference = fs.readFileSync(
  path.join(root, 'docs', 'extension-development', 'AI_COMPONENT_AUTHORING_REFERENCE.md'),
  'utf8',
);
const componentCatalog = fs.readFileSync(
  path.join(root, 'docs', 'extension-development', 'BUILTIN_COMPONENT_CATALOG.html'),
  'utf8',
);
const documentedSdkFunctions = [
  'get_project_context',
  'list_project_files',
  'stat_project_file',
  'resolve_project_file',
  'mutate_project_files',
  'get_project_metadata',
  'set_project_metadata',
  'put_blob',
  'open_blob',
  'delete_blob',
  'get_storage_directory',
];
for (const functionName of documentedSdkFunctions) {
  if (!new RegExp(`^def ${functionName}\\(`, 'm').test(sdkSource)) {
    throw new Error(`Python SDK is missing documented function ${functionName}`);
  }
  if (!interfaceReference.includes(`\`${functionName}`) && !interfaceReference.includes(`\`${functionName}(`)) {
    throw new Error(`Human interface reference is missing SDK function ${functionName}`);
  }
  if (!aiReference.includes(`${functionName}(`)) {
    throw new Error(`AI authoring reference is missing SDK function ${functionName}`);
  }
}

const builtinComponents = fs.readFileSync(
  path.join(root, 'src-tauri', 'src', 'platform', 'builtin_components.rs'),
  'utf8',
);
for (const requiredFragment of ['pmc.blendio.inspect', 'command: "inspect"', 'Capability::ProjectFilesRead']) {
  if (!builtinComponents.includes(requiredFragment)) {
    throw new Error(`BlenderIO public contract drifted: missing ${requiredFragment}`);
  }
}
const fileOperationCommands = [
  'project.describe', 'directory.list', 'entry.stat', 'entry.exists', 'entry.search',
  'file.read', 'file.write', 'file.hash', 'directory.create', 'entry.copy', 'entry.move',
  'entry.rename', 'entry.delete', 'batch.execute', 'stream.open-read', 'stream.read',
  'stream.open-write', 'stream.write', 'stream.commit', 'stream.abort', 'external.select',
  'external.grant-directory', 'external.list-grants', 'external.revoke-grant', 'external.import',
  'external.export', 'cache.status', 'cache.query', 'cache.invalidate',
  'cache.refresh-directory', 'cache.rebuild-project', 'watcher.status',
];
for (const command of fileOperationCommands) {
  if (!builtinComponents.includes(`"${command}"`)) {
    throw new Error(`File Operations public contract drifted: missing command ${command}`);
  }
  if (!interfaceReference.includes(`\`${command}\``) || !componentCatalog.includes(command)) {
    throw new Error(`File Operations documentation is missing command ${command}`);
  }
}
for (const requiredFragment of [
  'FILE_OPERATIONS_COMPONENT_ID: &str = "nexora.file-operations"',
  'required_capabilities_for_command',
  'UiExtensionPointContribution',
  'nexora.project-manager.project-workspace',
]) {
  if (!builtinComponents.includes(requiredFragment) && !fs.readFileSync(path.join(root, 'src-tauri', 'src', 'platform', 'file_operations.rs'), 'utf8').includes(requiredFragment)) {
    throw new Error(`R14.5 component contract is missing ${requiredFragment}`);
  }
}
for (const capability of [
  'project.files.read', 'project.files.write', 'filesystem.external.read',
  'filesystem.external.write', 'filesystem.dialog.open', 'cache.inspect', 'cache.maintain',
]) {
  if (!interfaceReference.includes(capability) || !componentCatalog.includes(capability)) {
    throw new Error(`File Operations documentation is missing Capability ${capability}`);
  }
}
for (const requiredText of [
  'requiredCapabilities', 'capabilities=[', 'external.select', 'uiExtensions', 'allowSession',
]) {
  if (!aiReference.includes(requiredText)) {
    throw new Error(`AI authoring reference is missing ${requiredText}`);
  }
}
if (aiReference.includes('One command has at most one `requiredCapability`')) {
  throw new Error('AI authoring reference still documents the obsolete single-Capability rule');
}
if (componentCatalog.includes('合同状态 / 来源') || componentCatalog.includes('${escapeHtml(component.source)}')) {
  throw new Error('Component catalog must not render internal source paths');
}
for (const requiredStatus of ['stable-2.8.5', 'experimental', 'planned-r14.5', 'reserved-r17']) {
  if (!interfaceReference.includes(requiredStatus) || !aiReference.includes(requiredStatus)) {
    throw new Error(`Extension references must define interface status ${requiredStatus}`);
  }
}

const cargo = process.platform === 'win32' ? 'cargo.exe' : 'cargo';
const rustCheck = spawnSync(
  cargo,
  ['test', '--manifest-path', path.join(contractRoot, 'Cargo.toml'), '--quiet'],
  { cwd: root, encoding: 'utf8' },
);
if (rustCheck.status !== 0) {
  process.stdout.write(rustCheck.stdout ?? '');
  process.stderr.write(rustCheck.stderr ?? '');
  throw new Error(`Rust platform contract tests failed with exit code ${rustCheck.status}`);
}

console.log('Nexora platform contract v1 checks passed.');
console.log(`Validated ${cases.length} schemas, ${cases.length} fixtures, ${componentExamplePaths.length} component examples, ${presentationExamplePaths.length} presentation examples, SDK docs, Capability parity, BlenderIO, and Rust semantic rules.`);

function readJson(filePath) {
  return JSON.parse(fs.readFileSync(filePath, 'utf8'));
}

function findEnumContaining(value, needle) {
  if (Array.isArray(value)) {
    if (value.includes(needle) && value.every((item) => typeof item === 'string')) return value;
    for (const item of value) {
      const found = findEnumContaining(item, needle);
      if (found) return found;
    }
  } else if (value && typeof value === 'object') {
    if (Array.isArray(value.enum) && value.enum.includes(needle)) return value.enum;
    for (const item of Object.values(value)) {
      const found = findEnumContaining(item, needle);
      if (found) return found;
    }
  }
  return null;
}

function assertSameSet(leftLabel, left, rightLabel, right) {
  const leftSet = new Set(left);
  const rightSet = new Set(right);
  const missing = [...rightSet].filter((value) => !leftSet.has(value));
  const extra = [...leftSet].filter((value) => !rightSet.has(value));
  if (missing.length || extra.length) {
    throw new Error(
      `${leftLabel} and ${rightLabel} differ. Missing: ${missing.join(', ') || '-'}; extra: ${extra.join(', ') || '-'}`,
    );
  }
}
