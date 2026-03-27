import { FileDetailsPanel } from './FileDetailsView';
import { useProjectStore } from '../../stores/projectStore';

export function FileDetail() {
  const { selectedFiles, files, searchResults, searchQuery, fileTags, tags } = useProjectStore();

  const selectedPaths = Array.from(selectedFiles);
  const displayFiles = searchQuery ? searchResults : files;
  const selectedFile = selectedPaths.length === 1
    ? displayFiles.find((file) => file.path === selectedPaths[0]) || files.find((file) => file.path === selectedPaths[0]) || null
    : null;

  const fileTagIds = selectedFile ? (fileTags.get(selectedFile.path) || []) : [];
  const fileTagList = tags.filter((tag) => fileTagIds.includes(tag.id));

  return (
    <FileDetailsPanel
      file={selectedFile}
      fileTagList={fileTagList}
      selectedCount={selectedPaths.length}
    />
  );
}
