import { invoke } from '@tauri-apps/api/core';

export interface ScannedProject {
  path: string;
  name: string;
  hasPmCenter: boolean;
}

/**
 * 扫描项目根目录，查找带 .pm_center 的项目
 * @param rootPath 根目录路径
 * @returns 项目列表
 */
export async function scanProjectsRoot(rootPath: string): Promise<ScannedProject[]> {
  return invoke('scan_projects_root', { rootPath });
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
