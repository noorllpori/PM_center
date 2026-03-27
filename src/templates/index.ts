export interface FolderTemplate {
  name: string;
  children: FolderTemplate[];
}

export interface ProjectTemplate {
  id: string;
  name: string;
  description: string;
  folders: FolderTemplate[];
}

// 标准渲染项目模板
export const standardRenderTemplate: ProjectTemplate = {
  id: 'standard-render',
  name: '标准渲染项目',
  description: '包含完整流程的渲染项目结构',
  folders: [
    {
      name: '01_assets',
      children: [
        { name: 'models', children: [] },
        { name: 'textures', children: [] },
        { name: 'references', children: [] },
        { name: 'hdri', children: [] },
      ],
    },
    {
      name: '02_scenes',
      children: [
        { name: 'work', children: [] },
        { name: 'publish', children: [] },
        { name: 'backup', children: [] },
      ],
    },
    {
      name: '03_renders',
      children: [
        { name: 'preview', children: [] },
        { name: 'draft', children: [] },
        { name: 'final', children: [] },
      ],
    },
    {
      name: '04_compositing',
      children: [
        { name: 'ae_projects', children: [] },
        { name: 'nuke_scripts', children: [] },
        { name: 'output', children: [] },
      ],
    },
    {
      name: '05_docs',
      children: [
        { name: 'storyboards', children: [] },
        { name: 'notes', children: [] },
      ],
    },
  ],
};

// 简单项目模板
export const simpleTemplate: ProjectTemplate = {
  id: 'simple',
  name: '简单项目',
  description: '最简化的项目结构',
  folders: [
    { name: 'assets', children: [] },
    { name: 'scenes', children: [] },
    { name: 'renders', children: [] },
  ],
};

// VFX项目模板
export const vfxTemplate: ProjectTemplate = {
  id: 'vfx',
  name: 'VFX项目',
  description: '视觉特效制作项目',
  folders: [
    {
      name: '01_plate',
      children: [
        { name: 'original', children: [] },
        { name: 'proxy', children: [] },
      ],
    },
    {
      name: '02_track',
      children: [
        { name: 'matchmove', children: [] },
        { name: 'stabilize', children: [] },
      ],
    },
    {
      name: '03_fx',
      children: [
        { name: 'sim', children: [] },
        { name: 'cache', children: [] },
      ],
    },
    {
      name: '04_lighting',
      children: [
        { name: 'work', children: [] },
        { name: 'publish', children: [] },
      ],
    },
    {
      name: '05_comp',
      children: [
        { name: 'scripts', children: [] },
        { name: 'prerenders', children: [] },
        { name: 'output', children: [] },
      ],
    },
  ],
};

export const templates: ProjectTemplate[] = [
  standardRenderTemplate,
  simpleTemplate,
  vfxTemplate,
];

// 根据模板创建文件夹结构
export async function createProjectFromTemplate(
  rootPath: string,
  template: ProjectTemplate
): Promise<void> {
  const { invoke } = await import('@tauri-apps/api/core');
  
  for (const folder of template.folders) {
    await createFolderRecursive(rootPath, folder, invoke);
  }
}

async function createFolderRecursive(
  parentPath: string,
  folder: FolderTemplate,
  invoke: any
): Promise<void> {
  const folderPath = `${parentPath}/${folder.name}`;
  
  await invoke('create_directory', { path: folderPath });
  
  for (const child of folder.children) {
    await createFolderRecursive(folderPath, child, invoke);
  }
}
