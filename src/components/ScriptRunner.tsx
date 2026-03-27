import { useState, useEffect } from 'react';
import { useScriptStore } from '../stores/scriptStore';
import { useProjectStore } from '../stores/projectStore';
import { Play, Square, Terminal, Package, RefreshCw, Plus, Trash2, FileCode } from 'lucide-react';
import { Script, EnvType } from '../types';

export function ScriptRunner() {
  const {
    envs,
    selectedEnv,
    scripts,
    detectEnvs,
    selectEnv,
    runScript,
    runScriptById,
    addScript,
    deleteScript,
    updateScript,
    loadBuiltinScripts,
  } = useScriptStore();
  
  const { projectPath } = useProjectStore();
  
  const [selectedScript, setSelectedScript] = useState<Script | null>(null);
  const [output, setOutput] = useState<string>('');
  const [isRunning, setIsRunning] = useState(false);
  const [showNewScript, setShowNewScript] = useState(false);
  const [newScript, setNewScript] = useState({
    name: '',
    description: '',
    code: '',
    category: 'Custom',
  });

  useEffect(() => {
    loadBuiltinScripts();
    detectEnvs();
  }, []);

  const handleRunScript = async () => {
    if (!selectedScript || !selectedEnv) return;

    setIsRunning(true);
    setOutput('Running...\n');

    try {
      let result;
      
      if (selectedScript.is_builtin && selectedScript.id === 'builtin_001') {
        // 特殊处理 Blender 文件解析
        result = { success: false, stdout: '', stderr: 'Please use file context menu', exit_code: 1 };
      } else {
        result = await runScriptById(selectedScript.id, {
          path: projectPath || '',
        });
      }

      setOutput(prev => prev + '\n' + (result.stdout || result.stderr));
      
      if (!result.success) {
        setOutput(prev => prev + '\n[Error] Exit code: ' + result.exit_code);
      }
    } catch (error) {
      setOutput(prev => prev + '\n[Error] ' + String(error));
    } finally {
      setIsRunning(false);
    }
  };

  const handleRunCustom = async () => {
    if (!selectedEnv || !newScript.code) return;

    setIsRunning(true);
    setOutput('Running custom script...\n');

    try {
      const result = await runScript(newScript.code, projectPath || undefined);
      setOutput(result.stdout || result.stderr);
    } catch (error) {
      setOutput('[Error] ' + String(error));
    } finally {
      setIsRunning(false);
    }
  };

  const categories = [...new Set(scripts.map(s => s.category))];

  return (
    <div className="h-full flex flex-col bg-white dark:bg-gray-900">
      {/* 工具栏 */}
      <div className="flex items-center justify-between px-3 py-2 border-b border-gray-200 dark:border-gray-700">
        <div className="flex items-center gap-2">
          <Terminal className="w-4 h-4 text-gray-500" />
          <span className="font-medium text-sm">脚本运行器</span>
        </div>
        
        <div className="flex items-center gap-2">
          {/* Python 环境选择 */}
          <select
            value={selectedEnv?.python_path || ''}
            onChange={(e) => {
              const env = envs.find(env => env.python_path === e.target.value);
              if (env) selectEnv(env);
            }}
            className="text-xs px-2 py-1 border border-gray-300 dark:border-gray-600 rounded"
          >
            <option value="">选择 Python 环境</option>
            {envs.map(env => (
              <option key={env.python_path} value={env.python_path}>
                {env.env_type === EnvType.Blender ? '🟠' : '🐍'} {env.version}
              </option>
            ))}
          </select>
          
          <button
            onClick={() => detectEnvs()}
            className="p-1 text-gray-500 hover:text-gray-700"
            title="刷新环境"
          >
            <RefreshCw className="w-3 h-3" />
          </button>
        </div>
      </div>

      <div className="flex-1 flex overflow-hidden">
        {/* 脚本列表 */}
        <div className="w-56 border-r border-gray-200 dark:border-gray-700 overflow-auto">
          <div className="p-2">
            <button
              onClick={() => setShowNewScript(true)}
              className="w-full flex items-center justify-center gap-1 px-3 py-1.5 text-xs
                         bg-blue-600 text-white rounded hover:bg-blue-700"
            >
              <Plus className="w-3 h-3" />
              新建脚本
            </button>
          </div>
          
          {categories.map(category => (
            <div key={category} className="mb-2">
              <div className="px-2 py-1 text-xs font-medium text-gray-500 uppercase">
                {category}
              </div>
              {scripts
                .filter(s => s.category === category)
                .map(script => (
                  <div
                    key={script.id}
                    onClick={() => {
                      setSelectedScript(script);
                      setShowNewScript(false);
                    }}
                    className={`
                      flex items-center gap-2 px-2 py-1.5 text-sm cursor-pointer
                      ${selectedScript?.id === script.id
                        ? 'bg-blue-50 dark:bg-blue-900/20 border-r-2 border-blue-500'
                        : 'hover:bg-gray-50 dark:hover:bg-gray-800'
                      }
                    `}
                  >
                    <FileCode className="w-3 h-3 text-gray-400" />
                    <span className="flex-1 truncate">{script.name}</span>
                    {!script.is_builtin && (
                      <button
                        onClick={(e) => {
                          e.stopPropagation();
                          deleteScript(script.id);
                        }}
                        className="text-gray-400 hover:text-red-500"
                      >
                        <Trash2 className="w-3 h-3" />
                      </button>
                    )}
                  </div>
                ))}
            </div>
          ))}
        </div>

        {/* 脚本编辑/运行区 */}
        <div className="flex-1 flex flex-col">
          {showNewScript ? (
            <>
              {/* 新建脚本表单 */}
              <div className="p-3 border-b border-gray-200 dark:border-gray-700 space-y-2">
                <input
                  type="text"
                  value={newScript.name}
                  onChange={(e) => setNewScript(s => ({ ...s, name: e.target.value }))}
                  placeholder="脚本名称"
                  className="w-full px-2 py-1 text-sm border border-gray-300 rounded"
                />
                <input
                  type="text"
                  value={newScript.description}
                  onChange={(e) => setNewScript(s => ({ ...s, description: e.target.value }))}
                  placeholder="描述"
                  className="w-full px-2 py-1 text-sm border border-gray-300 rounded"
                />
                <textarea
                  value={newScript.code}
                  onChange={(e) => setNewScript(s => ({ ...s, code: e.target.value }))}
                  placeholder="Python 代码..."
                  className="w-full h-32 px-2 py-1 text-sm font-mono border border-gray-300 rounded resize-none"
                  spellCheck={false}
                />
                <div className="flex gap-2">
                  <button
                    onClick={() => {
                      addScript({
                        ...newScript,
                        env_type: EnvType.System,
                      });
                      setShowNewScript(false);
                      setNewScript({ name: '', description: '', code: '', category: 'Custom' });
                    }}
                    className="px-3 py-1 text-xs bg-blue-600 text-white rounded"
                  >
                    保存
                  </button>
                  <button
                    onClick={() => setShowNewScript(false)}
                    className="px-3 py-1 text-xs text-gray-600 hover:bg-gray-100 rounded"
                  >
                    取消
                  </button>
                </div>
              </div>
            </>
          ) : selectedScript ? (
            <>
              {/* 脚本详情 */}
              <div className="p-3 border-b border-gray-200 dark:border-gray-700">
                <h3 className="font-medium text-sm">{selectedScript.name}</h3>
                <p className="text-xs text-gray-500 mt-1">{selectedScript.description}</p>
                
                <div className="flex gap-2 mt-2">
                  <button
                    onClick={handleRunScript}
                    disabled={isRunning || !selectedEnv}
                    className="flex items-center gap-1 px-3 py-1 text-xs
                               bg-green-600 text-white rounded hover:bg-green-700
                               disabled:opacity-50"
                  >
                    {isRunning ? (
                      <Square className="w-3 h-3" />
                    ) : (
                      <Play className="w-3 h-3" />
                    )}
                    {isRunning ? '运行中...' : '运行'}
                  </button>
                </div>
              </div>
              
              {!selectedScript.is_builtin && (
                <div className="p-3 border-b border-gray-200 dark:border-gray-700">
                  <textarea
                    value={selectedScript.code}
                    onChange={(e) => updateScript(selectedScript.id, { code: e.target.value })}
                    className="w-full h-32 px-2 py-1 text-sm font-mono border border-gray-300 rounded resize-none"
                    spellCheck={false}
                  />
                </div>
              )}
            </>
          ) : (
            <div className="flex-1 flex items-center justify-center text-gray-400 text-sm">
              选择或创建一个脚本
            </div>
          )}

          {/* 输出区域 */}
          <div className="flex-1 bg-gray-900 text-gray-100 p-3 overflow-auto font-mono text-xs">
            {output || '输出将显示在这里...'}
          </div>
        </div>
      </div>
    </div>
  );
}
