import type { LucideIcon } from 'lucide-react';
import {
  Box,
  Boxes,
  FolderOpen,
  Globe2,
  History,
  Info,
  MonitorUp,
  Puzzle,
  RefreshCw,
  SlidersHorizontal,
  Wrench,
} from 'lucide-react';
import type {
  SettingsNavigationIconKey,
  SettingsSectionRendererId,
} from '../../features/contributionRegistry';

export interface TrustedSettingsSectionImplementation {
  host: 'SettingsPanel';
  implementationId: string;
}

export const SETTINGS_SECTION_IMPLEMENTATIONS: Record<
  SettingsSectionRendererId,
  TrustedSettingsSectionImplementation
> = {
  'builtin.session-runtime.settings': {
    host: 'SettingsPanel',
    implementationId: 'renderSessionSettings',
  },
  'builtin.desktop-integration.settings': {
    host: 'SettingsPanel',
    implementationId: 'renderDesktopIntegrationSettings',
  },
  'builtin.local-web-console.settings': {
    host: 'SettingsPanel',
    implementationId: 'LocalWebConsoleSettingsSection',
  },
  'builtin.project-resources.global-exclusions-settings': {
    host: 'SettingsPanel',
    implementationId: 'renderGlobalExcludeRules',
  },
  'builtin.automation-runtime.global-settings': {
    host: 'SettingsPanel',
    implementationId: 'renderGlobalAutomationSettings',
  },
  'builtin.external-tools.settings': {
    host: 'SettingsPanel',
    implementationId: 'renderToolSettings',
  },
  'builtin.project-manager.history-settings': {
    host: 'SettingsPanel',
    implementationId: 'renderProjectHistorySettings',
  },
  'builtin.settings-center.component-settings-global': {
    host: 'SettingsPanel',
    implementationId: 'ComponentSchemaSettingsSection.global',
  },
  'builtin.settings-center.component-settings-project': {
    host: 'SettingsPanel',
    implementationId: 'ComponentSchemaSettingsSection.project',
  },
  'core.settings.about-settings': {
    host: 'SettingsPanel',
    implementationId: 'renderAboutSettings',
  },
  'builtin.project-resources.project-rules-settings': {
    host: 'SettingsPanel',
    implementationId: 'renderProjectRulesSettings',
  },
  'builtin.automation-runtime.project-settings': {
    host: 'SettingsPanel',
    implementationId: 'renderProjectPluginSettings',
  },
};

const SETTINGS_NAVIGATION_ICONS: Record<SettingsNavigationIconKey, LucideIcon> = {
  about: Info,
  automation: Puzzle,
  components: Boxes,
  desktop: MonitorUp,
  exclusions: FolderOpen,
  history: RefreshCw,
  platform: Box,
  session: History,
  sliders: SlidersHorizontal,
  tools: Wrench,
  'web-console': Globe2,
};

export function hasSettingsSectionImplementation(rendererId: SettingsSectionRendererId) {
  return Object.prototype.hasOwnProperty.call(SETTINGS_SECTION_IMPLEMENTATIONS, rendererId);
}

export function getSettingsSectionImplementationIds() {
  return Object.keys(SETTINGS_SECTION_IMPLEMENTATIONS);
}

export function getSettingsNavigationIcon(iconKey: SettingsNavigationIconKey) {
  return SETTINGS_NAVIGATION_ICONS[iconKey];
}
