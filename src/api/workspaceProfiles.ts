import { invoke } from '@tauri-apps/api/core';
import type {
  CreateWorkspaceProfileRequest,
  SaveWorkspaceProfileRequest,
  WorkspaceProfileDraftValidation,
  WorkspaceProfileMutationResult,
  WorkspaceProfileRuntimeSnapshot,
  WorkspaceProfileSwitchPreview,
  WorkspaceProfileSwitchResult,
} from '../types/workspaceProfileRuntime';
import type { WorkspaceProfileV1 } from '../types/platform';

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

export const getWorkspaceProfileDocument = (profileId: string) =>
  invoke<WorkspaceProfileV1>('get_workspace_profile_document', { profileId });

export const validateWorkspaceProfileDraft = (profile: WorkspaceProfileV1) =>
  invoke<WorkspaceProfileDraftValidation>('validate_workspace_profile_draft', { profile });

export const createWorkspaceProfile = (request: CreateWorkspaceProfileRequest) =>
  invoke<WorkspaceProfileMutationResult>('create_workspace_profile', { request });

export const saveWorkspaceProfile = (request: SaveWorkspaceProfileRequest) =>
  invoke<WorkspaceProfileMutationResult>('save_workspace_profile', { request });

export const previewWorkspaceProfileSwitch = (request: WorkspaceProfileSwitchContext) =>
  invoke<WorkspaceProfileSwitchPreview>('preview_workspace_profile_switch', { request });

export const switchWorkspaceProfile = (
  request: WorkspaceProfileSwitchContext & { expectedCurrentProfileId: string },
) => invoke<WorkspaceProfileSwitchResult>('switch_workspace_profile', { request });

export const finalizeWorkspaceProfileSwitch = (transactionId: string) =>
  invoke<WorkspaceProfileRuntimeSnapshot>('finalize_workspace_profile_switch', { transactionId });

export const rollbackWorkspaceProfileSwitch = (transactionId: string) =>
  invoke<WorkspaceProfileRuntimeSnapshot>('rollback_workspace_profile_switch', { transactionId });
