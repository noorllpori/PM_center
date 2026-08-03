import type { ModuleManifestV1 } from './platform';

export type PlatformModuleState =
  | 'disabled'
  | 'resolving'
  | 'starting'
  | 'running'
  | 'stopping'
  | 'blocked'
  | 'error'
  | 'restart-required';

export type PlatformModuleHealthLevel = 'unknown' | 'healthy' | 'degraded' | 'unhealthy';
export type PlatformModuleStopStrategy = 'graceful' | 'cascade' | 'force';

export interface PlatformModuleHealth {
  level: PlatformModuleHealthLevel;
  message: string;
  checkedAt: number | null;
}

export interface PlatformModuleErrorRecord {
  code: string;
  message: string;
  details: string[];
  occurredAt: number;
}

export interface PlatformResourceDiagnostic {
  id: string;
  moduleId: string;
  kind: string;
  label: string;
  createdAt: number;
  details: Record<string, string>;
  sequence: number;
}

export interface PlatformDependencyDiagnostic {
  id: string;
  required: boolean;
  versionRequirement: string;
  installed: boolean;
  compatible: boolean;
  state: PlatformModuleState | null;
}

export interface PlatformModuleDiagnostic {
  manifest: ModuleManifestV1;
  state: PlatformModuleState;
  desiredEnabled: boolean;
  health: PlatformModuleHealth;
  dependencies: PlatformDependencyDiagnostic[];
  dependents: string[];
  resources: PlatformResourceDiagnostic[];
  lastError: PlatformModuleErrorRecord | null;
  startedAt: number | null;
  updatedAt: number;
  diagnostic: boolean;
}

export interface PlatformModuleRuntimeOverview {
  modules: PlatformModuleDiagnostic[];
  resourceCount: number;
  persistencePath: string;
  previousShutdownClean: boolean;
  startupNotice: PlatformModuleErrorRecord | null;
}

export interface PlatformDisablePreview {
  moduleId: string;
  canDisableGracefully: boolean;
  runningDependents: string[];
  resources: PlatformResourceDiagnostic[];
  message: string;
}

export interface PlatformDiagnosticResult {
  action: string;
  success: boolean;
  iterations: number;
  leakedResources: number;
  activeTasks: number;
  message: string;
}

export interface PlatformModuleCommandError {
  code?: string;
  moduleId?: string | null;
  message?: string;
  details?: string[];
}
