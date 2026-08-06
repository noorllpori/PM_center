import type {
  ComponentManifestV1,
  ComponentRuntime,
  PageTemplateContribution,
  ShellTemplateContribution,
  ThemePresetContribution,
} from './platform';

export type ComponentInstallSource = 'bundled' | 'local' | 'marketplace';
export type ComponentOperationStatus =
  | 'starting'
  | 'running'
  | 'completed'
  | 'failed'
  | 'cancelled'
  | 'timed-out';

export interface ComponentWorkerSummary {
  componentId: string;
  pid: number;
  startedAt: number;
  status: string;
}

export interface InstalledComponentSummary {
  manifest: ComponentManifestV1;
  source: ComponentInstallSource;
  packagePath?: string | null;
  removable: boolean;
  hostAdapter?: string | null;
  activeOperationCount: number;
  worker?: ComponentWorkerSummary | null;
}

export interface ComponentOperationSummary {
  operationId: string;
  componentId: string;
  componentVersion: string;
  command: string;
  runtime: ComponentRuntime;
  status: ComponentOperationStatus;
  startedAt: number;
  finishedAt?: number | null;
  durationMs?: number | null;
  pid?: number | null;
  message: string;
  logPath?: string | null;
}

export interface PresentationTemplateOwner {
  componentId: string;
  componentName: string;
  componentVersion: string;
}

export interface PresentationTemplateCatalog {
  shellTemplates: Array<{ template: ShellTemplateContribution; owner: PresentationTemplateOwner }>;
  pageTemplates: Array<{ template: PageTemplateContribution; owner: PresentationTemplateOwner }>;
  themePresets: Array<{ preset: ThemePresetContribution; owner: PresentationTemplateOwner }>;
}

export interface ComponentRuntimeOverview {
  rootPath: string;
  statePath: string;
  installedComponents: InstalledComponentSummary[];
  availableBundledComponents: ComponentManifestV1[];
  activeOperations: ComponentOperationSummary[];
  recentOperations: ComponentOperationSummary[];
  templates: PresentationTemplateCatalog;
  legacyPythonActionCompatible: boolean;
  componentHostAvailable: boolean;
  componentHostPath?: string | null;
}

export interface ComponentPackageInspection {
  packagePath: string;
  valid: boolean;
  componentId?: string | null;
  componentName?: string | null;
  componentVersion?: string | null;
  fileCount: number;
  totalBytes: number;
  packageDigest?: string | null;
  warnings: string[];
}

export interface ComponentRuntimeCommandError {
  code?: string;
  message?: string;
  details?: string[];
}
