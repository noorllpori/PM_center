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

console.log('PM Center platform contract v1 checks passed.');
console.log(`Validated ${cases.length} schemas, ${cases.length} fixtures, and Rust semantic rules.`);

function readJson(filePath) {
  return JSON.parse(fs.readFileSync(filePath, 'utf8'));
}
