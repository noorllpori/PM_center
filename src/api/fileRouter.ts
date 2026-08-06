import { invoke } from '@tauri-apps/api/core';

export type FileIntent =
  | 'open'
  | 'open-internal'
  | 'open-system'
  | 'preview'
  | 'edit'
  | 'inspect'
  | 'thumbnail'
  | 'extract-metadata';

export interface FileRouteDiagnostic {
  code: string;
  message: string;
}

export interface FileRoutePlan {
  routeId: string;
  path: string;
  intent: FileIntent;
  accepted: boolean;
  fallbackToSystem: boolean;
  handlerId?: string | null;
  componentId?: string | null;
  handlerName?: string | null;
  target: 'workspace' | 'component' | 'system' | 'none';
  workspaceTarget?: string | null;
  diagnostics: FileRouteDiagnostic[];
}

export const routeFileIntent = (path: string, intent: FileIntent = 'open') =>
  invoke<FileRoutePlan>('route_file_intent', {
    request: { path, intent },
  });
