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

export interface FileRouteCandidate {
  handlerId: string;
  handlerName: string;
  componentId: string;
  componentName: string;
  priority: number;
  workspaceTarget?: string | null;
  eligible: boolean;
  selected: boolean;
  reasons: FileRouteDiagnostic[];
}

export interface FileRouteBindingResolution {
  id: string;
  scope: string;
  handler: string;
  behavior: 'fallback' | 'strict' | string;
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
  candidates: FileRouteCandidate[];
  binding?: FileRouteBindingResolution | null;
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
  storagePaths: Array<{ scope: 'global' | 'profile' | 'project' | string; path: string; available: boolean }>;
  context: FileRoutingScopeContext;
}

export interface FileRoutingScopeContext {
  projectPath?: string;
  profileId?: string;
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

export const getFileRoutingSnapshotForContext = (context: FileRoutingScopeContext) =>
  invoke<FileRoutingSnapshot>('get_file_routing_snapshot', { context });

export const setFileAssociationBinding = (binding: FileAssociationBinding) =>
  invoke<void>('set_file_association_binding', { request: { binding } });

export const removeFileAssociationBinding = (bindingId: string, context?: FileRoutingScopeContext) =>
  invoke<void>('remove_file_association_binding', { request: { bindingId, context } });

export const getFileRouteTrace = (routeId: string) =>
  invoke<FileRoutePlan>('get_file_route_trace', { routeId });
