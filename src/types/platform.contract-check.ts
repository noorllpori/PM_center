import type {
  ComponentManifestV1,
  ModuleManifestV1,
  PackageHeaderV1,
  WorkspaceProfileV1,
  WorkflowManifestV1,
} from './platform';

// These compile-time examples keep the public TypeScript names aligned with the v1 fixtures.
export const platformContractTypeExamples = {
  module: {
    schemaVersion: 1,
    id: 'render.farm-controller',
    name: '渲染农场控制',
    version: '1.0.0',
    apiVersion: '1',
    scope: 'global',
    capabilities: ['render.queue.write'],
  } satisfies ModuleManifestV1,
  component: {
    schemaVersion: 1,
    id: 'media.file-analyzer',
    name: '媒体文件分析',
    version: '1.0.0',
    apiVersion: '1',
    runtime: 'native-process',
    role: 'service',
    distribution: 'marketplace',
    uiMode: 'contributed',
    platforms: ['windows-x64'],
    entry: 'bin/windows-x64/analyzer.exe',
    contributes: {
      settingsSections: [{
        id: 'media.file-analyzer.general-settings',
        title: '分析设置',
        scope: 'global',
        fields: [{
          id: 'parallelism',
          label: '并行分析数',
          type: 'integer',
          defaultValue: 2,
          minimum: 1,
          maximum: 8,
        }],
      }],
    },
  } satisfies ComponentManifestV1,
  profile: {
    schemaVersion: 1,
    id: 'example.profile',
    name: '示例装配方案',
    enabledModules: [{ id: 'project.files', versionRequirement: '^1.0' }],
    enabledComponents: [{ id: 'media.file-analyzer', versionRequirement: '^1.0' }],
  } satisfies WorkspaceProfileV1,
  workflow: {
    schemaVersion: 1,
    id: 'example.workflow',
    name: '示例工作流',
    version: '1.0.0',
    trigger: { kind: 'manual' },
    nodes: [{ id: 'scan', nodeType: 'media.find-files' }],
  } satisfies WorkflowManifestV1,
  package: {
    magic: 'PMC_PACKAGE',
    schemaVersion: 1,
    formatVersion: 1,
    kind: 'profile',
    packageId: 'example.profile',
    createdAt: 0,
    producerVersion: '2.8.4',
    payload: {
      path: 'payload/profile.json',
      digest: {
        algorithm: 'blake3',
        value: 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
      },
    },
  } satisfies PackageHeaderV1,
};
