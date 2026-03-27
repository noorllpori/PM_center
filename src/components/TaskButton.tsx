import { useState, useEffect } from 'react';
import { Terminal } from 'lucide-react';
import { TaskPanel } from './TaskPanel';
import { useTaskStore, initTaskEventListeners } from '../stores/taskStore';

export function TaskButton() {
  const [isOpen, setIsOpen] = useState(false);
  const { stats } = useTaskStore();

  // 全局快捷键 Ctrl+B
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.ctrlKey && e.key === 'b') {
        e.preventDefault();
        setIsOpen(true);
      }
      if (e.key === 'Escape' && isOpen) {
        setIsOpen(false);
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [isOpen]);

  // 初始化事件监听
  useEffect(() => {
    initTaskEventListeners();
  }, []);

  // 计算运行中任务数
  const runningCount = stats.running;

  return (
    <>
      <button
        onClick={() => setIsOpen(true)}
        className="flex items-center gap-1.5 px-3 py-1.5 text-sm
                   bg-gradient-to-r from-purple-500 to-indigo-600 
                   hover:from-purple-600 hover:to-indigo-700
                   text-white rounded-lg shadow-sm
                   transition-all hover:shadow-md"
        title="任务中心 (Ctrl+B)"
      >
        <Terminal className="w-4 h-4" />
        <span className="hidden sm:inline">任务</span>
        {runningCount > 0 && (
          <span className="ml-1 px-1.5 py-0.5 bg-white/20 rounded-full text-xs">
            {runningCount}
          </span>
        )}
        <span className="text-xs opacity-70">Ctrl+B</span>
      </button>

      <TaskPanel isOpen={isOpen} onClose={() => setIsOpen(false)} />
    </>
  );
}
