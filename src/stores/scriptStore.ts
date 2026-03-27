import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';
import { PythonEnv, ScriptResult, Script, EnvType } from '../types';

interface ScriptState {
  // Python 环境
  envs: PythonEnv[];
  selectedEnv: PythonEnv | null;
  
  // 脚本
  scripts: Script[];
  runningScripts: Map<string, boolean>;
  
  // 操作
  detectEnvs: () => Promise<void>;
  selectEnv: (env: PythonEnv) => void;
  
  // 脚本执行
  runScript: (script: string, workingDir?: string) => Promise<ScriptResult>;
  runScriptById: (scriptId: string, params?: Record<string, unknown>) => Promise<ScriptResult>;
  runBlenderScript: (blendFile: string, script: string) => Promise<ScriptResult>;
  
  // 脚本管理
  addScript: (script: Omit<Script, 'id' | 'is_builtin'>) => void;
  deleteScript: (id: string) => void;
  updateScript: (id: string, updates: Partial<Script>) => void;
  
  // 内置脚本
  loadBuiltinScripts: () => void;
}

// 内置脚本库
const builtinScripts: Script[] = [
  {
    id: 'builtin_001',
    name: '获取 Blender 文件信息',
    description: '解析 .blend 文件，获取场景、相机、分辨率等信息',
    code: '', // 特殊处理，调用专用命令
    env_type: EnvType.Blender,
    category: 'Blender',
    is_builtin: true,
  },
  {
    id: 'builtin_002',
    name: '列出目录文件',
    description: '列出指定目录下的所有文件',
    code: `import os
import json

dir_path = r"{{path}}"
files = []

for f in os.listdir(dir_path):
    full_path = os.path.join(dir_path, f)
    files.append({
        "name": f,
        "is_dir": os.path.isdir(full_path),
        "size": os.path.getsize(full_path) if os.path.isfile(full_path) else 0
    })

print(json.dumps(files, indent=2, ensure_ascii=False))`,
    env_type: EnvType.System,
    category: '文件操作',
    is_builtin: true,
  },
  {
    id: 'builtin_003',
    name: '图片批量重命名',
    description: '按序列重命名图片文件',
    code: `import os
import re

dir_path = r"{{path}}"
pattern = r"{{pattern}}"  # 例如: render_*.png
prefix = r"{{prefix}}"    # 例如: final_
ext = r"{{ext}}"         # 例如: png

files = sorted([f for f in os.listdir(dir_path) if f.endswith(ext)])

for i, old_name in enumerate(files, 1):
    new_name = f"{prefix}{i:04d}.{ext}"
    os.rename(
        os.path.join(dir_path, old_name),
        os.path.join(dir_path, new_name)
    )
    print(f"Renamed: {old_name} -> {new_name}")

print(f"Done! Renamed {len(files)} files.")`,
    env_type: EnvType.System,
    category: '文件操作',
    is_builtin: true,
  },
  {
    id: 'builtin_004',
    name: '检查序列帧完整性',
    description: '检查序列帧是否有缺失',
    code: `import os
import re

dir_path = r"{{path}}"
pattern = r"{{pattern}}"  # 例如: render_####.png

# 提取现有帧
files = os.listdir(dir_path)
frame_numbers = []

# 解析 pattern 中的 # 数量
hash_count = pattern.count('#')
regex_pattern = pattern.replace('#' * hash_count, r'(\d{' + str(hash_count) + '})')
regex_pattern = regex_pattern.replace('.', r'\.')

for f in files:
    match = re.match(regex_pattern, f)
    if match:
        frame_numbers.append(int(match.group(1)))

if not frame_numbers:
    print("No matching files found!")
else:
    frame_numbers.sort()
    start, end = frame_numbers[0], frame_numbers[-1]
    expected = set(range(start, end + 1))
    actual = set(frame_numbers)
    missing = sorted(expected - actual)
    
    print(f"Frame range: {start} - {end}")
    print(f"Total frames: {len(frame_numbers)}")
    print(f"Missing frames: {len(missing)}")
    if missing:
        print(f"Missing: {missing[:20]}{'...' if len(missing) > 20 else ''}")`,
    env_type: EnvType.System,
    category: '序列帧',
    is_builtin: true,
  },
  {
    id: 'builtin_005',
    name: '生成缩略图',
    description: '使用 Pillow 生成图片缩略图',
    code: `from PIL import Image
import os

input_dir = r"{{input_dir}}"
output_dir = r"{{output_dir}}"
size = {{size}}  # 例如: 256

os.makedirs(output_dir, exist_ok=True)

for f in os.listdir(input_dir):
    if f.lower().endswith(('.png', '.jpg', '.jpeg')):
        img_path = os.path.join(input_dir, f)
        img = Image.open(img_path)
        img.thumbnail((size, size))
        
        output_path = os.path.join(output_dir, f"thumb_{f}")
        img.save(output_path)
        print(f"Created: {output_path}")`,
    env_type: EnvType.System,
    category: '图像处理',
    is_builtin: true,
  },
];

export const useScriptStore = create<ScriptState>((set, get) => ({
  // 初始状态
  envs: [],
  selectedEnv: null,
  scripts: [],
  runningScripts: new Map(),

  // 检测 Python 环境
  detectEnvs: async () => {
    try {
      const envs = await invoke<PythonEnv[]>('detect_python_envs');
      set({ envs });
      
      // 自动选择第一个可用环境
      if (envs.length > 0 && !get().selectedEnv) {
        set({ selectedEnv: envs[0] });
      }
    } catch (error) {
      console.error('Failed to detect Python envs:', error);
    }
  },

  // 选择环境
  selectEnv: (env: PythonEnv) => {
    set({ selectedEnv: env });
  },

  // 运行脚本代码
  runScript: async (script: string, workingDir?: string) => {
    const { selectedEnv } = get();
    if (!selectedEnv) {
      throw new Error('No Python environment selected');
    }

    const result = await invoke<ScriptResult>('run_python_script', {
      envType: selectedEnv.env_type,
      pythonPath: selectedEnv.python_path,
      script,
      workingDir,
      envVars: {},
    });

    return result;
  },

  // 运行脚本（通过 ID）
  runScriptById: async (scriptId: string, params?: Record<string, unknown>) => {
    const script = get().scripts.find(s => s.id === scriptId);
    if (!script) {
      throw new Error('Script not found');
    }

    // 替换模板参数
    let code = script.code;
    if (params) {
      for (const [key, value] of Object.entries(params)) {
        code = code.replace(new RegExp(`{{${key}}}`, 'g'), String(value));
      }
    }

    set(state => {
      const running = new Map(state.runningScripts);
      running.set(scriptId, true);
      return { runningScripts: running };
    });

    try {
      const result = await get().runScript(code);
      return result;
    } finally {
      set(state => {
        const running = new Map(state.runningScripts);
        running.delete(scriptId);
        return { runningScripts: running };
      });
    }
  },

  // 运行 Blender 专用脚本
  runBlenderScript: async (blendFile: string, script: string) => {
    const { envs } = get();
    const blenderEnv = envs.find(e => e.env_type === EnvType.Blender);
    
    if (!blenderEnv) {
      throw new Error('Blender not found');
    }

    return await invoke<ScriptResult>('run_python_script', {
      envType: EnvType.Blender,
      pythonPath: blenderEnv.python_path,
      script,
      workingDir: null,
      envVars: {},
    });
  },

  // 添加脚本
  addScript: (script) => {
    const newScript: Script = {
      ...script,
      id: `script_${Date.now()}`,
      is_builtin: false,
    };
    set(state => ({
      scripts: [...state.scripts, newScript],
    }));
  },

  // 删除脚本
  deleteScript: (id: string) => {
    set(state => ({
      scripts: state.scripts.filter(s => s.id !== id),
    }));
  },

  // 更新脚本
  updateScript: (id: string, updates: Partial<Script>) => {
    set(state => ({
      scripts: state.scripts.map(s =>
        s.id === id ? { ...s, ...updates } : s
      ),
    }));
  },

  // 加载内置脚本
  loadBuiltinScripts: () => {
    set({ scripts: builtinScripts });
  },
}));
