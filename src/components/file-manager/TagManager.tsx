import { useState } from 'react';
import { useProjectStoreShallow } from '../../stores/projectStore';
import { Plus, X, Tag } from 'lucide-react';

const PRESET_COLORS = [
  '#f5222d', '#fa541c', '#fa8c16', '#faad14',
  '#52c41a', '#13c2c2', '#1890ff', '#2f54eb',
  '#722ed1', '#eb2f96', '#8c8c8c', '#595959',
];

export function TagManager() {
  const { tags, selectedFiles, addTag, deleteTag, addTagToFile, removeTagFromFile, fileTags } = useProjectStoreShallow((state) => ({
    tags: state.tags,
    selectedFiles: state.selectedFiles,
    addTag: state.addTag,
    deleteTag: state.deleteTag,
    addTagToFile: state.addTagToFile,
    removeTagFromFile: state.removeTagFromFile,
    fileTags: state.fileTags,
  }));
  const [isCreating, setIsCreating] = useState(false);
  const [newTagName, setNewTagName] = useState('');
  const [selectedColor, setSelectedColor] = useState(PRESET_COLORS[6]);

  const selectedPaths = Array.from(selectedFiles);
  const selectedFileTags = selectedPaths.length === 1
    ? fileTags.get(selectedPaths[0]) || []
    : [];

  const handleCreateTag = () => {
    if (!newTagName.trim()) return;
    addTag({ name: newTagName.trim(), color: selectedColor });
    setNewTagName('');
    setIsCreating(false);
  };

  const handleToggleTag = (tagId: string) => {
    if (selectedPaths.length !== 1) return;
    
    const filePath = selectedPaths[0];
    if (selectedFileTags.includes(tagId)) {
      removeTagFromFile(filePath, tagId);
    } else {
      addTagToFile(filePath, tagId);
    }
  };

  if (selectedPaths.length === 0) {
    return (
      <div className="h-full flex items-center justify-center text-gray-400 text-sm p-4">
        选择文件以管理标签
      </div>
    );
  }

  return (
    <div className="h-full flex flex-col p-3">
      <div className="flex items-center justify-between mb-3">
        <h3 className="font-medium text-sm">
          {selectedPaths.length === 1 ? '文件标签' : `${selectedPaths.length} 个文件`}
        </h3>
        <button
          onClick={() => setIsCreating(true)}
          className="p-1 text-gray-600 hover:text-blue-600 hover:bg-blue-50 rounded"
        >
          <Plus className="w-4 h-4" />
        </button>
      </div>

      {isCreating && (
        <div className="mb-3 p-3 bg-gray-50 dark:bg-gray-800 rounded">
          <input
            type="text"
            value={newTagName}
            onChange={(e) => setNewTagName(e.target.value)}
            placeholder="标签名称"
            className="w-full px-2 py-1 text-sm border border-gray-300 rounded mb-2"
            autoFocus
          />
          <div className="flex gap-1 flex-wrap mb-2">
            {PRESET_COLORS.map(color => (
              <button
                key={color}
                onClick={() => setSelectedColor(color)}
                className={`w-5 h-5 rounded ${selectedColor === color ? 'ring-2 ring-offset-1 ring-gray-400' : ''}`}
                style={{ backgroundColor: color }}
              />
            ))}
          </div>
          <div className="flex gap-2">
            <button
              onClick={handleCreateTag}
              className="px-3 py-1 text-xs bg-blue-600 text-white rounded hover:bg-blue-700"
            >
              创建
            </button>
            <button
              onClick={() => setIsCreating(false)}
              className="px-3 py-1 text-xs text-gray-600 hover:bg-gray-200 rounded"
            >
              取消
            </button>
          </div>
        </div>
      )}

      <div className="flex-1 overflow-auto">
        <div className="space-y-1">
          {tags.map(tag => {
            const isSelected = selectedFileTags.includes(tag.id);
            return (
              <div
                key={tag.id}
                onClick={() => handleToggleTag(tag.id)}
                className={`
                  flex items-center gap-2 px-2 py-1.5 rounded cursor-pointer
                  ${isSelected ? 'bg-gray-100 dark:bg-gray-800' : 'hover:bg-gray-50 dark:hover:bg-gray-800/50'}
                `}
              >
                <span
                  className="w-3 h-3 rounded-full"
                  style={{ backgroundColor: tag.color }}
                />
                <span className="flex-1 text-sm">{tag.name}</span>
                {isSelected && <Tag className="w-3 h-3 text-gray-400" />}
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
