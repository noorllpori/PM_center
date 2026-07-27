import { invoke } from '@tauri-apps/api/core';

export interface ScannedProject {
  path: string;
  name: string;
  hasPmCenter: boolean;
}

export type ProjectLocationStatus =
  | 'ready'
  | 'missingDirectory'
  | 'notDirectory'
  | 'unreadable'
  | 'missingPmCenter'
  | 'invalidPmCenter'
  | 'invalidDataDb'
  | 'incompletePmCenter';

export interface ProjectLocationIssue {
  code: string;
  severity: 'warning' | 'error';
  message: string;
}

export interface ProjectLocationReport {
  projectPath: string;
  pmCenterPath: string;
  status: ProjectLocationStatus;
  missingItems: string[];
  issues: ProjectLocationIssue[];
  canInitialize: boolean;
  canRepair: boolean;
}

export interface ProjectLocationCandidate {
  path: string;
  name: string;
  matchReason: string;
}

/**
 * 扫描项目根目录，查找带 .pm_center 的项目
 * @param rootPath 根目录路径
 * @returns 项目列表
 */
export async function scanProjectsRoot(rootPath: string): Promise<ScannedProject[]> {
  return invoke('scan_projects_root', { rootPath });
}

export async function inspectProjectLocation(projectPath: string): Promise<ProjectLocationReport> {
  return invoke('inspect_project_location', { projectPath });
}

export async function findProjectLocationCandidates(
  projectPath: string,
  searchRoots: string[],
): Promise<ProjectLocationCandidate[]> {
  return invoke('find_project_location_candidates', { projectPath, searchRoots });
}

/**
 * 创建新项目
 * @param parentPath 父目录路径
 * @param projectName 项目名称
 * @returns 新项目路径
 */
export async function createProject(parentPath: string, projectName: string): Promise<string> {
  return invoke('create_project', { parentPath, projectName });
}
