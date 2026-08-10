import type { Capability, CapabilityRisk } from './platform';

export type CapabilitySubjectKind = 'module' | 'component';
export type CapabilityOperation = 'read' | 'write' | 'delete' | 'execute' | 'connect' | 'notify';
export type CapabilityScopeKind = 'none' | 'project' | 'library' | 'cache' | 'receive' | 'staging' | 'external';
export type CapabilityDecision = 'allowOnce' | 'allowSession' | 'allowAlways' | 'deny';
export type CapabilityRequestStatus = 'granted' | 'approval-required' | 'denied';

export interface CapabilityScopeRequest {
  kind: CapabilityScopeKind;
  rootPath: string | null;
  relativePath: string | null;
}

export interface ResolvedCapabilityScope extends CapabilityScopeRequest {
  resolvedPath: string | null;
}

export interface CapabilityTokenRequest {
  subjectKind: CapabilitySubjectKind;
  moduleId: string;
  componentId: string | null;
  capability: Capability;
  operation: CapabilityOperation;
  reason: string;
  scope: CapabilityScopeRequest;
}

export interface CapabilityApprovalRequest {
  requestId: string;
  subjectKind: CapabilitySubjectKind;
  subjectId: string;
  subjectName: string;
  subjectVersion: string;
  moduleId: string;
  moduleName: string;
  moduleVersion: string;
  capability: Capability;
  risk: CapabilityRisk;
  operation: CapabilityOperation;
  reason: string;
  scope: ResolvedCapabilityScope;
  createdAt: number;
  expiresAt: number;
}

export interface CapabilityToken {
  value: string;
  expiresAt: number;
}

export interface CapabilityTokenResponse {
  status: CapabilityRequestStatus;
  token: CapabilityToken | null;
  approval: CapabilityApprovalRequest | null;
  message: string;
}

export interface CapabilityGrantRecord {
  id: string;
  subjectKind: CapabilitySubjectKind;
  moduleId: string;
  moduleVersion: string;
  componentId: string | null;
  componentVersion: string | null;
  capability: Capability;
  risk: CapabilityRisk;
  operation: CapabilityOperation;
  scope: ResolvedCapabilityScope;
  createdAt: number;
  updatedAt: number;
  valid: boolean;
}

export interface CapabilityAuditRecord {
  id: number;
  occurredAt: number;
  requestId: string | null;
  subjectKind: CapabilitySubjectKind;
  subjectId: string;
  moduleId: string;
  capability: Capability;
  operation: CapabilityOperation;
  outcome: string;
  reasonCode: string;
  reason: string;
  scopeKind: CapabilityScopeKind;
  rootPath: string | null;
  relativePath: string | null;
  tokenId: string | null;
}

export interface CapabilityDiagnosticScenario {
  id: string;
  name: string;
  description: string;
  request: CapabilityTokenRequest;
}

export interface CapabilityGatewayOverview {
  databasePath: string;
  tokenTtlMs: number;
  activeTokenCount: number;
  grants: CapabilityGrantRecord[];
  pendingRequests: CapabilityApprovalRequest[];
  recentAudit: CapabilityAuditRecord[];
  diagnosticScenarios: CapabilityDiagnosticScenario[];
}

export interface CapabilityOperationResult {
  success: boolean;
  operation: CapabilityOperation;
  message: string;
  resolvedPath: string | null;
  bytesAffected: number;
}

export interface CapabilitySecurityDiagnosticResult {
  success: boolean;
  checks: number;
  passed: number;
  message: string;
}

export interface CapabilityGatewayCommandError {
  code?: string;
  message?: string;
  details?: string[];
}
