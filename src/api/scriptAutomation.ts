import { invoke } from '@tauri-apps/api/core';
import type { ProfileAutomationBinding } from '../types/platform';
import type {
  AutomationAttentionAction,
  AutomationBindingMutationResult,
  AutomationRun,
  AutomationRunFilter,
  AutomationRuntimeSnapshot,
  ComponentPackRequest,
  ComponentPackResult,
  DevelopmentComponentSnapshot,
  DevelopmentReloadResult,
  RemoveAutomationBindingRequest,
  SaveAutomationBindingRequest,
  ScriptComponentValidation,
  ScriptDevelopmentDocument,
  ScriptDevelopmentFile,
  ScriptSurfaceDocument,
  SigningKeyResult,
  StartAutomationRunRequest,
} from '../types/automation';

export const getAutomationRuntimeSnapshot = () =>
  invoke<AutomationRuntimeSnapshot>('get_automation_runtime_snapshot');

export const listAutomationRuns = (filter: AutomationRunFilter = {}) =>
  invoke<AutomationRun[]>('list_automation_runs', { filter });

export const getAutomationRun = (runId: string) =>
  invoke<AutomationRun>('get_automation_run', { runId });

export const startAutomationRun = (request: StartAutomationRunRequest) =>
  invoke<AutomationRun>('start_automation_run', { request });

export const cancelAutomationRun = (runId: string) =>
  invoke<AutomationRun>('cancel_automation_run', { runId });

export const retryAutomationRun = (runId: string) =>
  invoke<AutomationRun>('retry_automation_run', { runId });

export const resolveAutomationAttention = (runId: string, action: AutomationAttentionAction) =>
  invoke<AutomationRun>('resolve_automation_attention', { request: { runId, action } });

export const emitAutomationEvent = (
  event: string,
  payload: unknown,
  projectPath?: string | null,
  dedupeKey?: string | null,
) => invoke<number>('emit_automation_event', {
  request: { event, payload, projectPath, dedupeKey },
});

export const validateScriptComponent = (path: string) =>
  invoke<ScriptComponentValidation>('validate_script_component', { path });

export const getDevelopmentComponentSnapshot = () =>
  invoke<DevelopmentComponentSnapshot[]>('get_development_component_snapshot');

export const trustScriptDevelopmentDirectory = (path: string) =>
  invoke<string[]>('trust_script_development_directory', { path });

export const untrustScriptDevelopmentDirectory = (path: string) =>
  invoke<string[]>('untrust_script_development_directory', { path });

export const reloadScriptComponent = (componentId: string) =>
  invoke('reload_script_component', { componentId });

export const reloadDevelopmentComponents = (onlyDirty = true) =>
  invoke<DevelopmentReloadResult>('reload_development_components', { onlyDirty });

export const createScriptComponentTemplate = (request: {
  parentPath: string;
  componentId: string;
  name: string;
  includeSurface: boolean;
}) => invoke<string>('create_script_component_template', { request });

export const openScriptDevelopmentDirectoryInVSCode = (path: string) =>
  invoke<void>('open_script_development_directory_in_vscode', { path });

export const generateScriptSigningKey = (path: string) =>
  invoke<SigningKeyResult>('generate_script_signing_key', { path });

export const packageScriptComponent = (request: ComponentPackRequest) =>
  invoke<ComponentPackResult>('package_script_component', { request });

export const listScriptDevelopmentFiles = (path: string) =>
  invoke<ScriptDevelopmentFile[]>('list_script_development_files', { path });

export const readScriptDevelopmentFile = (path: string, relativePath: string) =>
  invoke<ScriptDevelopmentDocument>('read_script_development_file', { path, relativePath });

export const saveScriptDevelopmentFile = (request: {
  sourcePath: string;
  relativePath: string;
  content: string;
  expectedContentDigest?: string | null;
}) => invoke<ScriptDevelopmentDocument>('save_script_development_file', { request });

export const getScriptSurfaceDocument = (componentId: string, surfaceId: string) =>
  invoke<ScriptSurfaceDocument>('get_script_surface_document', { componentId, surfaceId });

export const listAutomationBindings = (profileId: string) =>
  invoke<ProfileAutomationBinding[]>('list_automation_bindings', { profileId });

export const saveAutomationBinding = (request: SaveAutomationBindingRequest) =>
  invoke<AutomationBindingMutationResult>('save_automation_binding', { request });

export const removeAutomationBinding = (request: RemoveAutomationBindingRequest) =>
  invoke<AutomationBindingMutationResult>('remove_automation_binding', { request });
