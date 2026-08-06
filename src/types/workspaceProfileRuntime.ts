import type {
  ComponentDistribution,
  ComponentSettingsSection,
  ComponentRole,
  ComponentRuntime,
  ComponentUiMode,
  ProfilePathVariable,
  ProfileToolAlias,
  WorkspaceProfileV1,
} from './platform';

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
  enabledComponentCount: number;
  effectiveComponentCount: number;
  pinnedToolCount: number;
  status: WorkspaceProfileDocumentStatus;
  issues: string[];
  path: string;
}

export interface WorkspaceProfileComponentSummary {
  id: string;
  name: string;
  description: string;
  version: string;
  runtime: ComponentRuntime;
  role: ComponentRole;
  distribution: ComponentDistribution;
  uiMode: ComponentUiMode;
  explicitEnabled: boolean;
  effectiveEnabled: boolean;
  requiredByModules: string[];
  requiredByComponents: string[];
  settingsSections: ComponentSettingsSection[];
}

export interface WorkspaceProfileDraftValidation {
  valid: boolean;
  selectedModuleCount: number;
  explicitComponentCount: number;
  effectiveComponentCount: number;
  components: WorkspaceProfileComponentSummary[];
  issues: WorkspaceProfileSwitchIssue[];
}

export interface WorkspaceProfileMutationResult {
  profile: WorkspaceProfileV1;
  validation: WorkspaceProfileDraftValidation;
  snapshot: WorkspaceProfileRuntimeSnapshot;
}

export interface CreateWorkspaceProfileRequest {
  name: string;
  description?: string;
  sourceProfileId?: string | null;
}

export interface SaveWorkspaceProfileRequest {
  profile: WorkspaceProfileV1;
  expectedRevision: number;
}

export interface ExportWorkspaceProfilePackageRequest {
  profileId: string;
  destinationPath: string;
}

export interface ExportWorkspacePackageRequest {
  profileId: string;
  destinationPath: string;
  variables?: Record<string, string>;
  openSurfaceIds?: string[];
  activeSurfaceId?: string | null;
}

export interface ProfilePackageExportResult {
  packageId: string;
  destinationPath: string;
  payloadDigest: string;
  sizeBytes: number;
}

export type ProfilePackageIssueSeverity = 'info' | 'warning' | 'error';

export interface ProfilePackageIssue {
  code: string;
  severity: ProfilePackageIssueSeverity;
  message: string;
  path?: string | null;
}

export interface ProfilePackageImportPreview {
  packagePath: string;
  packageId: string;
  producerVersion: string;
  profileName: string;
  description: string;
  suggestedName: string;
  payloadDigest: string;
  packageSizeBytes: number;
  moduleCount: number;
  componentCount: number;
  surfaceCount: number;
  widgetCount: number;
  pinnedToolCount: number;
  toolAliases: ProfileToolAlias[];
  pathVariables: ProfilePathVariable[];
  reusableBindingPresets: ProfileLocalBindingPreset[];
  missingModuleIds: string[];
  missingComponentIds: string[];
  issues: ProfilePackageIssue[];
  canImport: boolean;
}

export interface WorkspacePackageImportPreview extends ProfilePackageImportPreview {
  openSurfaceIds: string[];
  activeSurfaceId?: string | null;
  variables: Record<string, string>;
}

export type ProfileLocalBindingMode = 'automatic' | 'path';

export interface ProfileLocalBindingInput {
  id: string;
  mode: ProfileLocalBindingMode;
  path?: string | null;
}

export interface ProfileLocalBindingPreset {
  profileId: string;
  profileName: string;
  toolMappings: ProfileLocalBindingInput[];
  pathMappings: ProfileLocalBindingInput[];
}

export interface ImportWorkspaceProfilePackageRequest {
  packagePath: string;
  name: string;
  toolMappings?: ProfileLocalBindingInput[];
  pathMappings?: ProfileLocalBindingInput[];
}

export interface ImportWorkspacePackageRequest extends ImportWorkspaceProfilePackageRequest {}

export interface WorkspacePackageImportResult {
  mutation: WorkspaceProfileMutationResult;
  variables: Record<string, string>;
  openSurfaceIds: string[];
  activeSurfaceId?: string | null;
}

export interface WorkspaceProfileRuntimeSnapshot {
  currentProfile: WorkspaceProfileV1;
  profiles: WorkspaceProfileSummary[];
  components: WorkspaceProfileComponentSummary[];
  defaultProfileId: string;
  repositoryPath: string;
  statePath: string;
  journalPath: string;
  migration: WorkspaceProfileMigrationRecord;
  pendingSwitch?: WorkspaceProfilePendingSwitch | null;
  lastRecovery?: WorkspaceProfileRecoveryRecord | null;
}

export type WorkspaceProfileSwitchPhase = 'prepared' | 'modulesApplied' | 'profileCommitted';

export interface WorkspaceProfilePendingSwitch {
  schemaVersion: number;
  transactionId: string;
  fromProfileId: string;
  toProfileId: string;
  toProfileRevision: number;
  phase: WorkspaceProfileSwitchPhase;
  createdAt: number;
  updatedAt: number;
}

export type WorkspaceProfileRecoveryOutcome =
  | 'rolledBackInterruptedSwitch'
  | 'completedInterruptedSwitch'
  | 'reconciledRuntimeDrift';

export interface WorkspaceProfileRecoveryRecord {
  outcome: WorkspaceProfileRecoveryOutcome;
  profileId: string;
  transactionId?: string | null;
  recoveredAt: number;
  message: string;
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
  transactionId: string;
  preview: WorkspaceProfileSwitchPreview;
  snapshot: WorkspaceProfileRuntimeSnapshot;
  switchedAt: number;
}
