import { useState, useEffect } from 'react';
import { FileTree } from './FileTree';
import { FileList } from './FileList';
import { Toolbar } from './Toolbar';
import { ColumnSettings } from './ColumnSettings';
import { FileDetail } from './FileDetail';
import { ScriptRunner } from '../ScriptRunner';
import { ChangeLog } from '../ChangeLog';
import { TaskButton } from '../TaskButton';
import { LauncherButton } from '../Launcher';
import { WelcomeScreen } from '../WelcomeScreen';
import { P2PChat } from '../P2PChat';
import { PythonEnvManager } from '../PythonEnvManager';
import { useProjectStore } from '../../stores/projectStore';
import { useSettingsStore } from '../../stores/settingsStore';
import { Folder, Code, Clock, History, MessageCircle, Terminal } from 'lucide-react';

export function FileManager() {
  const { isInitialized, projectPath, projectName, setProject } = useProjectStore();
  const { 
    loadSettings, 
    addRecentProject, 
  } = useSettingsStore();
  
  const [activeTab, setActiveTab] = useState<'files' | 'scripts' | 'logs'>('files');
  const [isP2PChatOpen, setIsP2PChatOpen] = useState(false);
  const [isPythonEnvOpen, setIsPythonEnvOpen] = useState(false);

  // 初始化：加载设置
  useEffect(() => {
    loadSettings();
  }, []);

  // 当成功打开项目后，添加到历史
  useEffect(() => {
    if (isInitialized && projectPath && projectName) {
      addRecentProject(projectPath, projectName);
    }
  }, [isInitialized, projectPath, projectName]);

  // 处理打开项目
  const handleOpenProject = async (path: string) => {
    try {
      await setProject(path);
    } catch (error) {
      console.error('Failed to open project:', error);
    }
  };

  return (
    <div className="h-screen flex flex-col bg-gray-50 dark:bg-gray-900">
      {/* 顶部栏：工具栏 + 全局按钮 */}
      <div className="flex items-center border-b border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-900">
        <div className="flex-1">
          <Toolbar />
        </div>
        {/* 全局按钮区域 */}
        <div className="flex items-center gap-2 px-3 border-l border-gray-200 dark:border-gray-700">
          <button
            onClick={() => setIsPythonEnvOpen(true)}
            className="p-2 text-gray-500 hover:text-green-600 dark:text-gray-400 
                       dark:hover:text-green-400 hover:bg-green-50 dark:hover:bg-green-900/20 
                       rounded-lg transition-colors"
            title="Python 环境管理"
          >
            <Terminal className="w-5 h-5" />
          </button>
          <button
            onClick={() => setIsP2PChatOpen(true)}
            className="p-2 text-gray-500 hover:text-blue-600 dark:text-gray-400 
                       dark:hover:text-blue-400 hover:bg-blue-50 dark:hover:bg-blue-900/20 
                       rounded-lg transition-colors"
            title="局域网消息"
          >
            <MessageCircle className="w-5 h-5" />
          </button>
          <TaskButton />
          <LauncherButton />
        </div>
      </div>

      {/* Tab 切换 */}
      {isInitialized && (
        <div className="flex border-b border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-900">
          <button
            onClick={() => setActiveTab('files')}
            className={`
              flex items-center gap-1.5 px-4 py-2 text-sm font-medium
              ${activeTab === 'files'
                ? 'text-blue-600 border-b-2 border-blue-600'
                : 'text-gray-600 hover:text-gray-900'
              }
            `}
          >
            <Folder className="w-4 h-4" />
            文件
          </button>
          <button
            onClick={() => setActiveTab('scripts')}
            className={`
              flex items-center gap-1.5 px-4 py-2 text-sm font-medium
              ${activeTab === 'scripts'
                ? 'text-blue-600 border-b-2 border-blue-600'
                : 'text-gray-600 hover:text-gray-900'
              }
            `}
          >
            <Code className="w-4 h-4" />
            脚本
          </button>
          <button
            onClick={() => setActiveTab('logs')}
            className={`
              flex items-center gap-1.5 px-4 py-2 text-sm font-medium
              ${activeTab === 'logs'
                ? 'text-blue-600 border-b-2 border-blue-600'
                : 'text-gray-600 hover:text-gray-900'
              }
            `}
          >
            <History className="w-4 h-4" />
            日志
          </button>
        </div>
      )}

      {/* 主内容区 */}
      <div className="flex-1 flex overflow-hidden">
        {!isInitialized ? (
          <WelcomeScreen onOpenProject={handleOpenProject} />
        ) : activeTab === 'scripts' ? (
          <ScriptRunner />
        ) : activeTab === 'logs' ? (
          <ChangeLog />
        ) : (
          <>
            {/* 左侧：文件树 */}
            <div className="w-64 border-r border-gray-200 dark:border-gray-700 flex-shrink-0">
              <FileTree />
            </div>

            {/* 中间：文件列表 */}
            <div className="flex-1 overflow-hidden">
              <FileList />
            </div>

            {/* 右侧：文件详情 */}
            <div className="w-56 border-l border-gray-200 dark:border-gray-700 flex-shrink-0 bg-white dark:bg-gray-900">
              <FileDetail />
            </div>
          </>
        )}
      </div>

      {/* 列设置 */}
      {isInitialized && activeTab === 'files' && <ColumnSettings />}

      {/* P2P 聊天 */}
      <P2PChat 
        isOpen={isP2PChatOpen} 
        onClose={() => setIsP2PChatOpen(false)} 
      />

      {/* Python 环境管理 */}
      <PythonEnvManager 
        isOpen={isPythonEnvOpen} 
        onClose={() => setIsPythonEnvOpen(false)} 
      />
    </div>
  );
}
