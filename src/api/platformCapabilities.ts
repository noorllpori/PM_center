import { invoke } from '@tauri-apps/api/core';
import type {
  CapabilityDecision,
  CapabilityGatewayOverview,
  CapabilityOperationResult,
  CapabilitySecurityDiagnosticResult,
  CapabilityTokenRequest,
  CapabilityTokenResponse,
} from '../types/capabilityRuntime';

export const getCapabilityGatewayOverview = () =>
  invoke<CapabilityGatewayOverview>('get_capability_gateway_overview');

export const requestPlatformCapability = (request: CapabilityTokenRequest) =>
  invoke<CapabilityTokenResponse>('request_platform_capability', { request });

export const decidePlatformCapability = (requestId: string, decision: CapabilityDecision) =>
  invoke<CapabilityTokenResponse>('decide_platform_capability', {
    request: { requestId, decision },
  });

export const revokePlatformCapabilityGrant = (grantId: string) =>
  invoke<void>('revoke_platform_capability_grant', { grantId });

export const runPlatformCapabilityOperation = (
  token: string,
  request: CapabilityTokenRequest,
) => invoke<CapabilityOperationResult>('run_platform_capability_operation', { token, request });

export const runPlatformCapabilityDiagnostic = () =>
  invoke<CapabilitySecurityDiagnosticResult>('run_platform_capability_diagnostic');
