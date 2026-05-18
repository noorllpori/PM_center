// 文件信息
export type FileEntryKind = 'file' | 'manual_collection' | 'image_sequence';

export interface ImageSequenceInfo {
  id: string;
  virtual_path: string;
  directory_path: string;
  prefix: string;
  extension: string;
  padding: number;
  start_frame: number;
  end_frame: number;
  frame_count: number;
  missing_count: number;
  frames: Array<{
    frame: number;
    path: string;
    name: string;
  }>;
}

export interface FileInfo {
  name: string;
  path: string;
  is_dir: boolean;
  size: number;
  modified: string | null;
  created: string | null;
  extension: string | null;
  thumbnail: string | null;
  entry_kind?: FileEntryKind;
  virtual_path?: string;
  collection_id?: string;
  item_count?: number;
  sequence?: ImageSequenceInfo | null;
  collection_member_paths?: string[];
  directory_path?: string;
  created_at?: number;
  updated_at?: number;
}

export interface FileDetailsItem {
  label: string;
  value: string;
  details?: FileDetailsItemDetails | null;
  editKey?: string | null;
}

export interface BlenderSceneRenderEdit {
  resolutionX?: number | null;
  resolutionY?: number | null;
  frameStart?: number | null;
  frameEnd?: number | null;
  frameCurrent?: number | null;
  fps?: number | null;
  outputPath?: string | null;
}

export interface BlenderWriteOptions {
  backup?: boolean;
  threadCount?: number | null;
  zstdLevel?: number | null;
}

export interface BlenderWriteReport {
  path: string;
  backupPath: string | null;
  compression: 'none' | 'gzip' | 'zstd';
  patchCount: number;
  bytesChanged: number;
  threadCount: number;
  verified: boolean;
}

export type FileDetailsItemDetails =
  | {
      kind: 'textList';
      values: string[];
    }
  | {
      kind: 'records';
      columns: FileDetailsRecordColumn[];
      records: Record<string, unknown>[];
    };

export interface FileDetailsRecordColumn {
  key: string;
  label: string;
}

export interface FileDetailsSection {
  id: string;
  title: string;
  items: FileDetailsItem[];
}

export interface FileDetailsBasic {
  name: string;
  path: string;
  size: number;
  size_formatted: string;
  is_dir: boolean;
  created: string | null;
  modified: string | null;
  accessed: string | null;
  readonly: boolean;
  hidden: boolean;
  extension: string | null;
  mime: string | null;
  detected_kind: string;
  display_type: string;
}

export interface FileDetailsParser {
  id: string;
  source: string;
  status: string;
  warning: string | null;
}

export interface FileDetailsResponse {
  basic: FileDetailsBasic;
  parser: FileDetailsParser;
  sections: FileDetailsSection[];
}

export interface BlenderExternalDataSummary {
  images: BlenderExternalImage[];
  libraries: BlenderExternalLibrary[];
  texts: BlenderExternalText[];
  linkedIds: BlenderLinkedId[];
}

export interface BlenderPreview {
  width: number;
  height: number;
  rgba: number[];
}

export interface BlenderExternalImage {
  name: string;
  filepath: string | null;
  resolvedPath: string | null;
  packed: boolean;
  sourceCode: number;
  source: string;
  imageTypeCode: number;
  imageType: string;
  generatedWidth: number;
  generatedHeight: number;
  colorspace: string | null;
  libraryPath: string | null;
  isExternal: boolean;
}

export interface BlenderExternalLibrary {
  name: string;
  filepath: string | null;
  resolvedPath: string | null;
  packed: boolean;
}

export interface BlenderExternalText {
  name: string;
  filepath: string | null;
  resolvedPath: string | null;
  lineCount: number;
  isExternal: boolean;
  libraryPath: string | null;
}

export interface BlenderLinkedId {
  code: string;
  kind: string;
  name: string | null;
  libraryPath: string | null;
}

// 树节点
export interface TreeNode {
  name: string;
  path: string;
  is_dir: boolean;
  children: TreeNode[];
}

// 标签
export interface Tag {
  id: string;
  name: string;
  color: string;
}

// 文件标签
export interface FileTag {
  file_path: string;
  tag_id: string;
}

// 文件元数据
export interface FileMetadata {
  file_path: string;
  status: string | null;
  notes: string | null;
  custom_data: Record<string, unknown> | null;
}

// 项目信息
export interface ProjectInfo {
  name: string;
  path: string;
  root_path: string;
}

// 列配置
export interface ColumnConfig {
  key: string;
  title: string;
  width: number;
  visible: boolean;
  sortable: boolean;
  align?: 'left' | 'center' | 'right';
}

// 显示规则
export interface DisplayRule {
  id: string;
  name: string;
  condition: {
    field: 'name' | 'extension' | 'path' | 'tag' | 'status';
    operator: 'contains' | 'startsWith' | 'endsWith' | 'equals' | 'regex';
    value: string;
  };
  action: {
    type: 'highlight' | 'badge' | 'textColor' | 'icon';
    color?: string;
    icon?: string;
    label?: string;
  };
  enabled: boolean;
}

// 视图模式
export type ViewMode = 'list' | 'grid' | 'thumbnail';

// Python 环境类型
export enum EnvType {
  System = 'System',
  Embedded = 'Embedded',
  Blender = 'Blender',
  Custom = 'Custom',
}

export interface PythonEnv {
  python_path: string;
  env_type: EnvType;
  version: string;
}

export interface ScriptResult {
  success: boolean;
  stdout: string;
  stderr: string;
  exit_code: number | null;
}

// 脚本定义
export interface Script {
  id: string;
  name: string;
  description: string;
  code: string;
  env_type: EnvType;
  category: string;
  is_builtin: boolean;
}

// 文件变更记录
export interface FileChange {
  id: number;
  project_path: string;
  file_path: string;
  change_type: string; // created, modified, deleted
  file_size: number | null;
  timestamp: number;
  depth: number;
}

// 任务系统类型
export * from './task';
export * from './plugin';
export * from './externalDrag';
