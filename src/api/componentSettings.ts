import { invoke } from '@tauri-apps/api/core';
import type { JsonValue, SettingsScope } from '../types/platform';

export interface ComponentSettingsTarget {
  componentId: string;
  sectionId: string;
  scope: SettingsScope;
  projectPath?: string | null;
}

export interface ComponentSettingsSnapshot extends ComponentSettingsTarget {
  values: Record<string, JsonValue>;
  storagePath: string;
  updatedAt: number;
}

export const getComponentSettings = (request: ComponentSettingsTarget) =>
  invoke<ComponentSettingsSnapshot>('get_component_settings', { request });

export const saveComponentSettings = (
  request: ComponentSettingsTarget & { values: Record<string, JsonValue> },
) => invoke<ComponentSettingsSnapshot>('save_component_settings', {
  request: {
    componentId: request.componentId,
    sectionId: request.sectionId,
    scope: request.scope,
    projectPath: request.projectPath ?? null,
    values: request.values,
  },
});
