import type {
  AutomationCommandContribution,
  AutomationExecutionSemantics,
  ComponentManifestV1,
  JsonValue,
  ProfileAutomationBinding,
  ScriptSurfaceContribution,
} from './platform';
import type { WorkspaceProfileMutationResult } from './workspaceProfileRuntime';

export type AutomationRunStatus =
  | 'queued'
  | 'preparing'
  | 'running'
  | 'waiting-permission'
  | 'cancelling'
  | 'completed'
  | 'failed'
  | 'cancelled'
  | 'attention';

export interface AutomationRun {
  id: string;
  componentId: string;
  componentName: string;
  componentVersion: string;
  command: string;
  commandName: string;
  profileId: string;
  profileRevision: number;
  projectPath?: string | null;
  triggerKind: string;
  triggerId?: string | null;
  status: AutomationRunStatus;
  progress?: number | null;
  progressMessage?: string | null;
  executionSemantics: AutomationExecutionSemantics;
  attempt: number;
  maxAttempts: number;
  operationId?: string | null;
  contentDigest: string;
  input: JsonValue;
  output?: JsonValue | null;
  logs: string[];
  error?: string | null;
  permissionRequest?: Record<string, unknown> | null;
  attempts: AutomationRunAttempt[];
  attention?: AutomationAttention | null;
  createdAt: number;
  startedAt?: number | null;
  finishedAt?: number | null;
  updatedAt: number;
}

export interface AutomationRunAttempt {
  attempt: number;
  operationId: string;
  status: string;
  logs: string[];
  error?: string | null;
  startedAt: number;
  finishedAt?: number | null;
}

export interface AutomationAttention {
  id: string;
  kind: string;
  message: string;
  status: string;
  resolution?: string | null;
  createdAt: number;
  resolvedAt?: number | null;
}

export interface AutomationComponentSummary {
  componentId: string;
  componentName: string;
  componentVersion: string;
  commands: AutomationCommandContribution[];
  events: string[];
  surfaces: ScriptSurfaceContribution[];
}

export type AutomationBinding = ProfileAutomationBinding;

export interface ScriptDevelopmentProject {
  sourcePath: string;
  trusted: boolean;
  contentDigest?: string | null;
  manifest?: ComponentManifestV1 | null;
  files: ScriptDevelopmentFile[];
}

export interface ComponentPackRequest {
  sourcePath: string;
  destinationPath: string;
  keyPath: string;
  publisherId: string;
  publisherName: string;
  license?: string;
  producerVersion?: string;
}

export interface ComponentPackResult {
  componentId: string;
  componentVersion: string;
  destinationPath: string;
  contentDigest: string;
  fileCount: number;
}

export interface SigningKeyResult {
  path: string;
  publicKey: string;
}

export interface AutomationRuntimeSnapshot {
  running: boolean;
  databasePath: string;
  trustedDevelopmentDirectories: string[];
  activeCount: number;
  waitingPermissionCount: number;
  attentionCount: number;
  recentRuns: AutomationRun[];
  availableComponents: AutomationComponentSummary[];
  legacyWorkflowsExecutable: boolean;
}

export interface StartAutomationRunRequest {
  componentId: string;
  command: string;
  input?: JsonValue;
  projectPath?: string | null;
  triggerKind?: string;
  triggerId?: string | null;
  capabilityScope?: Record<string, unknown> | null;
}

export interface AutomationRunFilter {
  profileId?: string | null;
  projectPath?: string | null;
  status?: AutomationRunStatus | null;
  limit?: number;
}

export type AutomationAttentionAction =
  | 'allowOnce'
  | 'allowAlways'
  | 'deny'
  | 'retrySafe'
  | 'markFailed'
  | 'cancel';

export interface ScriptComponentValidation {
  valid: boolean;
  sourcePath: string;
  trusted: boolean;
  contentDigest?: string | null;
  manifest?: ComponentManifestV1 | null;
  warnings: string[];
  errors: string[];
}

export interface ScriptSurfaceDocument {
  componentId: string;
  surfaceId: string;
  title: string;
  nonce: string;
  allowedCommands: string[];
  html: string;
}

export interface ScriptDevelopmentFile {
  path: string;
  sizeBytes: number;
  modifiedAt: number;
}

export interface ScriptDevelopmentDocument {
  path: string;
  content: string;
  contentDigest: string;
}

export interface SaveAutomationBindingRequest {
  profileId: string;
  expectedRevision: number;
  binding: ProfileAutomationBinding;
}

export interface RemoveAutomationBindingRequest {
  profileId: string;
  expectedRevision: number;
  bindingId: string;
}

export type AutomationBindingMutationResult = WorkspaceProfileMutationResult;
