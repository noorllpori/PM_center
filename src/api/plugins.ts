import { invoke } from '@tauri-apps/api/core';
import type { PluginDescriptor, PluginDirectories } from '../types/plugin';

export async function listPlugins(projectPath?: string | null): Promise<PluginDescriptor[]> {
  return invoke('list_plugins', {
    projectPath: projectPath ?? null,
  });
}

export async function refreshPlugins(projectPath?: string | null): Promise<PluginDescriptor[]> {
  return invoke('refresh_plugins', {
    projectPath: projectPath ?? null,
  });
}

export async function setPluginEnabled(pluginKey: string, enabled: boolean): Promise<void> {
  await invoke('set_plugin_enabled', { pluginKey, enabled });
}

export async function getPluginDirs(projectPath?: string | null): Promise<PluginDirectories> {
  return invoke('get_plugin_dirs', {
    projectPath: projectPath ?? null,
  });
}
