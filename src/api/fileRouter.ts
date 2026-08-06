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

export interface FileAssociationBinding {
  id: string;
  scope: 'global' | 'profile' | 'project';
  extension?: string | null;
  mimeType?: string | null;
  intent: FileIntent;
  handler: string;
  behavior?: 'fallback' | 'strict';
  projectPath?: string | null;
  profileId?: string | null;
}

export interface FileRoutingSnapshot {
  bindings: FileAssociationBinding[];
  storagePath: string;
}

export const routeFileIntent = (
  path: string,
  intent: FileIntent = 'open',
  options?: { preferredHandlerId?: string; projectPath?: string; profileId?: string },
) =>
  invoke<FileRoutePlan>('route_file_intent', {
    request: { path, intent, ...options },
  });

export const getFileRoutingSnapshot = () =>
  invoke<FileRoutingSnapshot>('get_file_routing_snapshot');

export const setFileAssociationBinding = (binding: FileAssociationBinding) =>
  invoke<void>('set_file_association_binding', { request: { binding } });

export const removeFileAssociationBinding = (bindingId: string) =>
  invoke<void>('remove_file_association_binding', { bindingId });
