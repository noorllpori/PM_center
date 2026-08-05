import { invoke } from '@tauri-apps/api/core';
import type { PlatformModuleDiagnostic } from '../types/platformRuntime';
import { PLATFORM_MODULE_RUNTIME_CHANGED_EVENT } from './platformModules';

export const LOCAL_WEB_CONSOLE_MODULE_ID = 'builtin.local-web-console';
export const LOCAL_WEB_CONSOLE_TOOL_CONTRIBUTION_ID = 'builtin.local-web-console.tool';
export const LOCAL_WEB_SETTINGS_CHANGED_EVENT = 'pm-center:local-web-settings-changed';

export interface LocalWebConsoleConfig {
  preferredPort: number;
  allowSettingsWrite: boolean;
  allowRestart: boolean;
  allowExit: boolean;
}

export interface LocalWebConsoleStatus {
  running: boolean;
  address: string | null;
  launchUrl: string | null;
  startedAt: number | null;
  config: LocalWebConsoleConfig;
}

export interface LocalWebEditableSettings {
  autoOpenLastProject: boolean;
  confirmProjectTabClose: boolean;
  confirmFileTabClose: boolean;
  projectsRootDir: string | null;
}

export const getLocalWebConsoleStatus = () =>
  invoke<LocalWebConsoleStatus>('get_local_web_console_status');

export const updateLocalWebConsoleConfig = (config: LocalWebConsoleConfig) =>
  invoke<LocalWebConsoleStatus>('update_local_web_console_config', { request: config });

export const openLocalWebConsole = () => invoke<void>('open_local_web_console');

export const setLocalWebConsoleEnabled = async (enabled: boolean) => {
  const result = await invoke<PlatformModuleDiagnostic>('set_local_web_console_enabled', { enabled });
  if (typeof window !== 'undefined') {
    window.dispatchEvent(new Event(PLATFORM_MODULE_RUNTIME_CHANGED_EVENT));
  }
  return result;
};
