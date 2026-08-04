import { invoke } from '@tauri-apps/api/core';
import type {
  PlatformDiagnosticResult,
  PlatformDisablePreview,
  PlatformModuleDiagnostic,
  PlatformModuleRuntimeOverview,
  PlatformModuleStopStrategy,
} from '../types/platformRuntime';

export const PLATFORM_MODULE_RUNTIME_CHANGED_EVENT = 'pm-center:platform-module-runtime-changed';

function notifyPlatformModuleRuntimeChanged() {
  if (typeof window !== 'undefined') {
    window.dispatchEvent(new Event(PLATFORM_MODULE_RUNTIME_CHANGED_EVENT));
  }
}

export const getPlatformModuleRuntime = () =>
  invoke<PlatformModuleRuntimeOverview>('list_platform_modules');

export const getPlatformModule = (moduleId: string) =>
  invoke<PlatformModuleDiagnostic>('get_platform_module', { moduleId });

export const previewDisablePlatformModule = (moduleId: string) =>
  invoke<PlatformDisablePreview>('preview_disable_platform_module', { moduleId });

export const enablePlatformModule = async (moduleId: string) => {
  const result = await invoke<PlatformModuleDiagnostic>('enable_platform_module', { moduleId });
  notifyPlatformModuleRuntimeChanged();
  return result;
};

export const disablePlatformModule = (
  moduleId: string,
  strategy: PlatformModuleStopStrategy,
) => invoke<PlatformModuleDiagnostic>('disable_platform_module', { moduleId, strategy })
  .then((result) => {
    notifyPlatformModuleRuntimeChanged();
    return result;
  });

export const restartPlatformModule = async (moduleId: string) => {
  const result = await invoke<PlatformModuleDiagnostic>('restart_platform_module', { moduleId });
  notifyPlatformModuleRuntimeChanged();
  return result;
};

export const runPlatformModuleHealthCheck = (moduleId?: string) =>
  invoke<PlatformModuleDiagnostic[]>('run_platform_module_health_check', {
    moduleId: moduleId || null,
  });

export const configurePlatformModuleFailure = (
  moduleId: string,
  point: string,
  enabled: boolean,
) => invoke('configure_platform_module_failure', { request: { moduleId, point, enabled } });

export const getPlatformModuleFailureInjections = () =>
  invoke<Record<string, boolean>>('get_platform_module_failure_injections');

export const runPlatformCycleLeakTest = () =>
  invoke<PlatformDiagnosticResult>('run_platform_module_diagnostic', {
    action: 'cycleLeakTest',
  });
