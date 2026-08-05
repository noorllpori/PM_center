import type { JsonValue } from '../types/platform';
import {
  COMMAND_CONTRIBUTION_BY_ID,
  COMMAND_CONTRIBUTIONS,
  getContributionUnavailableReason,
  type ContributionRegistrySnapshot,
} from './contributionRegistry';

export type ContributionCommandHandler = (
  payload?: JsonValue,
) => Promise<void> | void;

export type ContributionCommandHandlers = Record<string, ContributionCommandHandler>;

const PROJECT_HOME_COMMAND_IDS = new Set(
  Object.values(COMMAND_CONTRIBUTIONS).map((definition) => definition.id),
);

export function getContributionCommandHandlerIds() {
  return Array.from(PROJECT_HOME_COMMAND_IDS);
}

export async function executeContributionCommand(
  snapshot: ContributionRegistrySnapshot,
  handlers: ContributionCommandHandlers,
  commandId: string,
  payload?: JsonValue,
) {
  const definition = COMMAND_CONTRIBUTION_BY_ID.get(commandId);
  if (!definition) {
    throw new Error(`命令贡献未注册：${commandId}`);
  }
  const unavailableReason = getContributionUnavailableReason(snapshot, definition);
  if (unavailableReason) {
    throw new Error(unavailableReason);
  }
  const handler = handlers[commandId];
  if (!handler) {
    throw new Error(`命令尚未绑定当前 Surface：${commandId}`);
  }
  await handler(payload);
}
