import { invoke } from '@tauri-apps/api/core';
import type { ProjectScript } from '../types/task';

/**
 * 获取项目脚本列表
 * @param projectPath 项目路径
 * @returns 脚本列表
 */
export async function getProjectScripts(projectPath: string): Promise<ProjectScript[]> {
  return invoke('get_project_scripts', { projectPath });
}
