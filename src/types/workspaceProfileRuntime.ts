import type { WorkspaceProfileV1 } from './platform';

export type WorkspaceProfileDocumentStatus = 'ready' | 'blocked' | 'invalid';

export interface WorkspaceProfileMigrationRecord {
  source: string;
  sourceVersion: string;
  createdAt: number;
  capturedModuleCount: number;
  capturedPinnedToolCount: number;
}

export interface WorkspaceProfileSummary {
  id: string;
  name: string;
  description: string;
  revision: number;
  current: boolean;
  enabledModuleCount: number;
  pinnedToolCount: number;
  status: WorkspaceProfileDocumentStatus;
  issues: string[];
  path: string;
}

export interface WorkspaceProfileRuntimeSnapshot {
  currentProfile: WorkspaceProfileV1;
  profiles: WorkspaceProfileSummary[];
  repositoryPath: string;
  statePath: string;
  migration: WorkspaceProfileMigrationRecord;
}

export interface WorkspaceProfileRuntimeCommandError {
  code?: string;
  message?: string;
  path?: string | null;
  details?: string[];
}

export type WorkspaceProfileSwitchIssueSeverity = 'info' | 'warning' | 'error';

export interface WorkspaceProfileSwitchIssue {
  code: string;
  severity: WorkspaceProfileSwitchIssueSeverity;
  category: string;
  message: string;
  moduleId?: string | null;
  contributionId?: string | null;
}

export interface WorkspaceProfileModuleChange {
  moduleId: string;
  name: string;
  version: string;
  currentState: string;
  currentDesiredEnabled: boolean;
  targetEnabled: boolean;
  resourceCount: number;
  resourceLabels: string[];
}

export interface WorkspaceProfileSwitchPreview {
  currentProfileId: string;
  targetProfileId: string;
  targetProfileName: string;
  targetProfileRevision: number;
  canSwitch: boolean;
  noChanges: boolean;
  requiresConfirmation: boolean;
  modulesToEnable: WorkspaceProfileModuleChange[];
  modulesToDisable: WorkspaceProfileModuleChange[];
  pinnedToolsBefore: string[];
  pinnedToolsAfter: string[];
  pinnedToolsAdded: string[];
  pinnedToolsRemoved: string[];
  contributionsToClose: string[];
  resourcesToRelease: number;
  issues: WorkspaceProfileSwitchIssue[];
}

export interface WorkspaceProfileSwitchResult {
  preview: WorkspaceProfileSwitchPreview;
  snapshot: WorkspaceProfileRuntimeSnapshot;
  switchedAt: number;
}
