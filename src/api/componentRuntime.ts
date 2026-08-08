import { invoke } from '@tauri-apps/api/core';
import type { ComponentManifestV1 } from '../types/platform';
import type {
  ComponentPackageInspection,
  ComponentInvocationResult,
  ComponentRuntimeOverview,
  PresentationTemplatePreview,
} from '../types/componentRuntime';

export const COMPONENT_OPERATION_EVENT = 'nexora:component-operation';

export const getComponentRuntimeOverview = () =>
  invoke<ComponentRuntimeOverview>('get_component_runtime_overview');

export const installComponentFromDirectory = (sourcePath: string) =>
  invoke<ComponentManifestV1>('install_component_from_directory', {
    request: { sourcePath },
  });

export const inspectComponentPackage = (packagePath: string) =>
  invoke<ComponentPackageInspection>('inspect_component_package', { packagePath });

export const getPresentationTemplatePreview = (componentId: string, templateId: string) =>
  invoke<PresentationTemplatePreview>('get_presentation_template_preview', {
    request: { componentId, templateId },
  });

export const installComponentFromPackage = (packagePath: string) =>
  invoke<ComponentManifestV1>('install_component_from_package', {
    request: { packagePath },
  });

export const trustComponentPackagePublisher = (packagePath: string) =>
  invoke<{ id: string; displayName: string; publicKey: string }>('trust_component_package_publisher', { packagePath });

export const uninstallComponent = (componentId: string) =>
  invoke<ComponentManifestV1>('uninstall_component', { componentId });

export const disableComponent = (componentId: string) =>
  invoke<ComponentManifestV1>('disable_component', { componentId });

export const enableComponent = (componentId: string) =>
  invoke<ComponentManifestV1>('enable_component', { componentId });

export const deleteComponent = (componentId: string) =>
  invoke<ComponentManifestV1>('delete_component', { componentId });

export const invokeComponentCommand = (request: {
  componentId: string;
  moduleId: string;
  command: string;
  input?: unknown;
  timeoutMs?: number;
}) => invoke<ComponentInvocationResult>('invoke_component_command', { request });

export const reinstallBundledComponent = (componentId: string) =>
  invoke<ComponentManifestV1>('reinstall_bundled_component', { componentId });

export const cancelComponentOperation = (operationId: string) =>
  invoke<void>('cancel_component_operation', { operationId });
