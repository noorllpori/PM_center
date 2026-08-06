import { invoke } from '@tauri-apps/api/core';
import type { ComponentManifestV1 } from '../types/platform';
import type { ComponentRuntimeOverview } from '../types/componentRuntime';

export const COMPONENT_OPERATION_EVENT = 'nexora:component-operation';

export const getComponentRuntimeOverview = () =>
  invoke<ComponentRuntimeOverview>('get_component_runtime_overview');

export const installComponentFromDirectory = (sourcePath: string) =>
  invoke<ComponentManifestV1>('install_component_from_directory', {
    request: { sourcePath },
  });

export const uninstallComponent = (componentId: string) =>
  invoke<ComponentManifestV1>('uninstall_component', { componentId });

export const reinstallBundledComponent = (componentId: string) =>
  invoke<ComponentManifestV1>('reinstall_bundled_component', { componentId });

export const cancelComponentOperation = (operationId: string) =>
  invoke<void>('cancel_component_operation', { operationId });
