import { useState } from 'react';
import { useProjectStoreShallow } from '../../stores/projectStore';
import { Settings, GripVertical, Eye, EyeOff } from 'lucide-react';

export function ColumnSettings() {
  const [isOpen, setIsOpen] = useState(false);
  const { columns, updateColumn, reorderColumns } = useProjectStoreShallow((state) => ({
    columns: state.columns,
    updateColumn: state.updateColumn,
    reorderColumns: state.reorderColumns,
  }));
  
  const [draggedIndex, setDraggedIndex] = useState<number | null>(null);

  const handleDragStart = (index: number) => {
    setDraggedIndex(index);
  };

  const handleDragOver = (e: React.DragEvent, index: number) => {
    e.preventDefault();
    if (draggedIndex === null || draggedIndex === index) return;

    const newColumns = [...columns];
    const dragged = newColumns[draggedIndex];
    newColumns.splice(draggedIndex, 1);
    newColumns.splice(index, 0, dragged);
    
    reorderColumns(newColumns.map(c => c.key));
    setDraggedIndex(index);
  };

  const handleDragEnd = () => {
    setDraggedIndex(null);
  };

  if (!isOpen) {
    return (
      <button
        onClick={() => setIsOpen(true)}
        className="fixed bottom-4 right-4 p-2 bg-white dark:bg-gray-800 
                   shadow-lg rounded-full hover:shadow-xl transition-shadow"
        title="列设置"
      >
        <Settings className="w-5 h-5 text-gray-600" />
      </button>
    );
  }

  return (
    <div className="fixed bottom-4 right-4 w-64 bg-white dark:bg-gray-800 
                    shadow-xl rounded-lg border border-gray-200 dark:border-gray-700">
      <div className="flex items-center justify-between p-3 border-b 
                      border-gray-200 dark:border-gray-700">
        <h3 className="font-medium text-sm">列设置</h3>
        <button
          onClick={() => setIsOpen(false)}
          className="text-gray-400 hover:text-gray-600"
        >
          ×
        </button>
      </div>
      
      <div className="p-2 max-h-64 overflow-auto">
        {columns.map((col, index) => (
          <div
            key={col.key}
            draggable
            onDragStart={() => handleDragStart(index)}
            onDragOver={(e) => handleDragOver(e, index)}
            onDragEnd={handleDragEnd}
            className="flex items-center gap-2 p-2 hover:bg-gray-50 
                       dark:hover:bg-gray-700 rounded cursor-move"
          >
            <GripVertical className="w-4 h-4 text-gray-400" />
            
            <button
              onClick={() => updateColumn(col.key, { visible: !col.visible })}
              className="text-gray-600 dark:text-gray-400"
            >
              {col.visible ? (
                <Eye className="w-4 h-4" />
              ) : (
                <EyeOff className="w-4 h-4" />
              )}
            </button>
            
            <span className={`flex-1 text-sm ${!col.visible ? 'text-gray-400' : ''}`}>
              {col.title}
            </span>
          </div>
        ))}
      </div>
      
      <div className="p-2 text-xs text-gray-400 border-t 
                      border-gray-200 dark:border-gray-700">
        拖拽调整顺序，点击眼睛图标显示/隐藏
      </div>
    </div>
  );
}
