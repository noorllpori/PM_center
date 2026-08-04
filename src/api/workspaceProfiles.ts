import { invoke } from '@tauri-apps/api/core';
import type {
  WorkspaceProfileRuntimeSnapshot,
  WorkspaceProfileSwitchPreview,
  WorkspaceProfileSwitchResult,
} from '../types/workspaceProfileRuntime';

export interface WorkspaceProfileSwitchContext {
  profileId: string;
  currentPinnedTools: string[];
  knownToolContributions: string[];
}

export const initializeWorkspaceProfileRuntime = (legacyPinnedTools: string[]) =>
  invoke<WorkspaceProfileRuntimeSnapshot>('initialize_workspace_profile_runtime', {
    request: { legacyPinnedTools },
  });

export const getWorkspaceProfileRuntime = () =>
  invoke<WorkspaceProfileRuntimeSnapshot>('get_workspace_profile_runtime');

export const previewWorkspaceProfileSwitch = (request: WorkspaceProfileSwitchContext) =>
  invoke<WorkspaceProfileSwitchPreview>('preview_workspace_profile_switch', { request });

export const switchWorkspaceProfile = (
  request: WorkspaceProfileSwitchContext & { expectedCurrentProfileId: string },
) => invoke<WorkspaceProfileSwitchResult>('switch_workspace_profile', { request });
