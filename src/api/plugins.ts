import { invoke } from '@tauri-apps/api/core';
import type { PluginDependencyInfo, PluginDescriptor, PluginDirectories } from '../types/plugin';

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

export async function inspectPluginDependencies(
  pluginKey: string,
  projectPath?: string | null,
): Promise<PluginDependencyInfo> {
  return invoke('inspect_plugin_dependencies', {
    pluginKey,
    projectPath: projectPath ?? null,
  });
}

export async function installPluginDependencies(
  pluginKey: string,
  projectPath?: string | null,
): Promise<PluginDependencyInfo> {
  return invoke('install_plugin_dependencies', {
    pluginKey,
    projectPath: projectPath ?? null,
  });
}

export async function removePluginDependencies(
  pluginKey: string,
  projectPath?: string | null,
): Promise<PluginDependencyInfo> {
  return invoke('remove_plugin_dependencies', {
    pluginKey,
    projectPath: projectPath ?? null,
  });
}

export async function updatePluginSettings(
  pluginKey: string,
  values: Record<string, unknown>,
  projectPath?: string | null,
): Promise<PluginDescriptor> {
  return invoke('update_plugin_settings', {
    pluginKey,
    values,
    projectPath: projectPath ?? null,
  });
}

export async function resetPluginSettings(
  pluginKey: string,
  projectPath?: string | null,
): Promise<PluginDescriptor> {
  return invoke('reset_plugin_settings', {
    pluginKey,
    projectPath: projectPath ?? null,
  });
}
