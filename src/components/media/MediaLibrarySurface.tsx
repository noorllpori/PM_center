import { useCallback, useEffect, useMemo, useState } from 'react';
import { convertFileSrc, invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import {
  Archive,
  ChevronLeft,
  FileImage,
  FileVideo,
  FolderOpen,
  LibraryBig,
  LoaderCircle,
  Music2,
  Plus,
  Search,
  Star,
  Tag,
} from 'lucide-react';
import { useUiStore } from '../../stores/uiStore';
import type {
  MediaCatalogItem,
  MediaCollection,
  MediaImportMode,
  MediaImportResult,
  MediaLibraryBookmark,
  MediaLibrarySnapshot,
} from '../../types/mediaLibrary';

type BrowserView = 'all' | 'collection' | 'tag';

function formatBytes(bytes: number) {
  if (!Number.isFinite(bytes) || bytes <= 0) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value.toFixed(unit === 0 ? 0 : 1)} ${units[unit]}`;
}

function formatTime(value: number | null) {
  return value ? new Date(value).toLocaleString() : '-';
}

function kindIcon(kind: string) {
  if (kind === 'video') return FileVideo;
  if (kind === 'audio') return Music2;
  return FileImage;
}

function previewSource(item: MediaCatalogItem | null) {
  if (!item || !item.primaryPath) return '';
  return convertFileSrc(item.primaryPath);
}

export function MediaLibrarySurface({ isActive }: { isActive: boolean }) {
  const showToast = useUiStore((state) => state.showToast);
  const [bookmarks, setBookmarks] = useState<MediaLibraryBookmark[]>([]);
  const [snapshot, setSnapshot] = useState<MediaLibrarySnapshot | null>(null);
  const [search, setSearch] = useState('');
  const [activeCollectionId, setActiveCollectionId] = useState<string | null>(null);
  const [activeTagId, setActiveTagId] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [isImporting, setIsImporting] = useState(false);
  const [newCollectionName, setNewCollectionName] = useState('');
  const [tagInput, setTagInput] = useState('');
  const [noteInput, setNoteInput] = useState('');
  const [importMode, setImportMode] = useState<MediaImportMode>('reference');
  const [showMobileDetail, setShowMobileDetail] = useState(false);

  const selected = useMemo(
    () => snapshot?.items.find((item) => item.id === selectedId) || null,
    [selectedId, snapshot?.items],
  );
  const activeCollection = snapshot?.collections.find((item) => item.id === activeCollectionId) || null;
  const activeTag = snapshot?.tags.find((item) => item.id === activeTagId) || null;
  const currentRoot = snapshot?.library.rootPath || null;

  const refreshBookmarks = useCallback(async () => {
    try {
      setBookmarks(await invoke<MediaLibraryBookmark[]>('list_media_libraries'));
    } catch (error) {
      showToast({ title: '读取媒体资料库失败', message: String(error), tone: 'error' });
    }
  }, [showToast]);

  const refreshSnapshot = useCallback(async (root: string, options?: { preserveSelection?: boolean }) => {
    setIsLoading(true);
    try {
      const next = await invoke<MediaLibrarySnapshot>('get_media_library_snapshot', {
        query: {
          libraryPath: root,
          search: search.trim() || null,
          collectionId: activeCollectionId,
          tagId: activeTagId,
          offset: 0,
          limit: 180,
        },
      });
      setSnapshot(next);
      setSelectedId((current) => (
        options?.preserveSelection && current && next.items.some((item) => item.id === current)
          ? current
          : next.items[0]?.id || null
      ));
    } catch (error) {
      showToast({ title: '读取媒体资料库失败', message: String(error), tone: 'error' });
    } finally {
      setIsLoading(false);
    }
  }, [activeCollectionId, activeTagId, search, showToast]);

  const openLibrary = useCallback(async (path: string) => {
    setIsLoading(true);
    try {
      const next = await invoke<MediaLibrarySnapshot>('open_media_library', { libraryPath: path });
      setSnapshot(next);
      setActiveCollectionId(null);
      setActiveTagId(null);
      setSearch('');
      setSelectedId(next.items[0]?.id || null);
      setShowMobileDetail(false);
      await refreshBookmarks();
    } catch (error) {
      showToast({ title: '打开媒体资料库失败', message: String(error), tone: 'error' });
    } finally {
      setIsLoading(false);
    }
  }, [refreshBookmarks, showToast]);

  useEffect(() => {
    if (isActive) void refreshBookmarks();
  }, [isActive, refreshBookmarks]);

  useEffect(() => {
    if (!currentRoot || !isActive) return;
    const timer = window.setTimeout(() => void refreshSnapshot(currentRoot, { preserveSelection: true }), 180);
    return () => window.clearTimeout(timer);
  }, [activeCollectionId, activeTagId, currentRoot, isActive, refreshSnapshot, search]);

  useEffect(() => {
    setNoteInput(selected?.note || '');
    setTagInput(selected?.tags.join(', ') || '');
  }, [selected?.id, selected?.note, selected?.tags]);

  const selectLibraryDirectory = async () => {
    const selectedPath = await open({ directory: true, multiple: false, title: '选择媒体资料库目录' });
    if (typeof selectedPath === 'string') await openLibrary(selectedPath);
  };

  const importMedia = async (directory: boolean) => {
    if (!currentRoot) return;
    const selectedPaths = await open({
      directory,
      multiple: true,
      title: directory ? '选择要收集的媒体目录' : '选择要收集的媒体文件',
      filters: directory ? undefined : [{ name: '媒体与参考文件', extensions: ['jpg', 'jpeg', 'png', 'webp', 'bmp', 'gif', 'tif', 'tiff', 'exr', 'hdr', 'mp4', 'mov', 'mkv', 'avi', 'webm', 'm4v', 'mp3', 'wav', 'flac', 'aac', 'm4a', 'ogg', 'blend', 'psd', 'ai', 'pdf'] }],
    });
    const paths = !selectedPaths ? [] : Array.isArray(selectedPaths) ? selectedPaths : [selectedPaths];
    if (paths.length === 0) return;
    setIsImporting(true);
    try {
      const result = await invoke<MediaImportResult>('import_media_items', {
        request: {
          libraryPath: currentRoot,
          paths,
          mode: importMode,
          collectionId: activeCollectionId,
          tagNames: tagInput.split(',').map((value) => value.trim()).filter(Boolean),
        },
      });
      const failedMessage = result.failed > 0 ? `，${result.failed} 个未导入` : '';
      showToast({
        title: '媒体收集完成',
        message: `新增 ${result.imported} 个，关联重复内容 ${result.duplicatesLinked} 个${failedMessage}`,
        tone: result.failed > 0 ? 'warning' : 'success',
      });
      await refreshSnapshot(currentRoot);
    } catch (error) {
      showToast({ title: '媒体收集失败', message: String(error), tone: 'error' });
    } finally {
      setIsImporting(false);
    }
  };

  const createCollection = async () => {
    if (!currentRoot || !newCollectionName.trim()) return;
    try {
      const collection = await invoke<MediaCollection>('create_media_collection', {
        request: { libraryPath: currentRoot, name: newCollectionName.trim(), color: null },
      });
      setNewCollectionName('');
      setActiveCollectionId(collection.id);
      await refreshSnapshot(currentRoot);
    } catch (error) {
      showToast({ title: '创建媒体集合失败', message: String(error), tone: 'error' });
    }
  };

  const saveMetadata = async () => {
    if (!currentRoot || !selected) return;
    try {
      await Promise.all([
        invoke('update_media_annotation', { request: { libraryPath: currentRoot, itemId: selected.id, note: noteInput, rating: selected.rating } }),
        invoke('set_media_item_tags', { request: { libraryPath: currentRoot, itemId: selected.id, tagNames: tagInput.split(',').map((value) => value.trim()).filter(Boolean) } }),
      ]);
      showToast({ title: '资料已保存', message: '备注和标签已写入当前媒体资料库', tone: 'success' });
      await refreshSnapshot(currentRoot, { preserveSelection: true });
    } catch (error) {
      showToast({ title: '保存资料失败', message: String(error), tone: 'error' });
    }
  };

  const setRating = async (rating: number) => {
    if (!currentRoot || !selected) return;
    try {
      await invoke('update_media_annotation', { request: { libraryPath: currentRoot, itemId: selected.id, note: noteInput, rating } });
      await refreshSnapshot(currentRoot, { preserveSelection: true });
    } catch (error) {
      showToast({ title: '保存评分失败', message: String(error), tone: 'error' });
    }
  };

  const selectItem = (item: MediaCatalogItem) => {
    setSelectedId(item.id);
    setShowMobileDetail(true);
  };

  const browserTitle = activeCollection ? activeCollection.name : activeTag ? `#${activeTag.name}` : '全部资料';

  return (
    <div className="flex h-full min-h-0 bg-white text-gray-900 dark:bg-gray-950 dark:text-gray-100">
      <aside className={`flex w-64 shrink-0 flex-col border-r border-gray-200 bg-gray-50 dark:border-gray-800 dark:bg-gray-900 ${showMobileDetail ? 'hidden md:flex' : 'flex'}`}>
        <div className="border-b border-gray-200 p-3 dark:border-gray-800">
          <div className="flex items-center gap-2"><LibraryBig className="h-5 w-5 text-teal-600" /><h2 className="truncate text-sm font-semibold">媒体资料库</h2></div>
          <button type="button" onClick={() => void selectLibraryDirectory()} className="mt-3 flex h-8 w-full items-center gap-2 rounded border border-gray-300 bg-white px-2.5 text-xs hover:bg-gray-100 dark:border-gray-700 dark:bg-gray-800 dark:hover:bg-gray-700"><FolderOpen className="h-3.5 w-3.5" />打开或新建资料库</button>
        </div>
        <div className="min-h-0 flex-1 overflow-auto p-2">
          <p className="px-2 pb-1 text-[10px] font-medium text-gray-400">最近资料库</p>
          <div className="space-y-1">{bookmarks.map((bookmark) => <button key={bookmark.rootPath} type="button" disabled={!bookmark.available} onClick={() => void openLibrary(bookmark.rootPath)} className={`flex w-full min-w-0 items-center gap-2 rounded px-2 py-1.5 text-left text-xs ${snapshot?.library.rootPath === bookmark.rootPath ? 'bg-teal-100 text-teal-900 dark:bg-teal-950/55 dark:text-teal-100' : 'hover:bg-gray-200/70 dark:hover:bg-gray-800'} disabled:opacity-45`}><LibraryBig className="h-3.5 w-3.5 shrink-0" /><span className="truncate">{bookmark.displayName}</span></button>)}</div>
          {snapshot ? <>
            <div className="mt-5 flex items-center justify-between px-2"><p className="text-[10px] font-medium text-gray-400">集合</p><span className="text-[10px] text-gray-400">{snapshot.collections.length}</span></div>
            <button type="button" onClick={() => { setActiveCollectionId(null); setActiveTagId(null); }} className={`mt-1 flex w-full items-center justify-between rounded px-2 py-1.5 text-xs ${!activeCollectionId && !activeTagId ? 'bg-teal-100 text-teal-900 dark:bg-teal-950/55 dark:text-teal-100' : 'hover:bg-gray-200/70 dark:hover:bg-gray-800'}`}><span>全部资料</span><span className="text-[10px] opacity-65">{snapshot.library.itemCount}</span></button>
            <div className="mt-1 flex gap-1 px-1"><input value={newCollectionName} onChange={(event) => setNewCollectionName(event.target.value)} onKeyDown={(event) => { if (event.key === 'Enter') void createCollection(); }} placeholder="新建集合" className="h-7 min-w-0 flex-1 rounded border border-gray-300 bg-white px-2 text-[11px] outline-none dark:border-gray-700 dark:bg-gray-800" /><button type="button" title="新建集合" onClick={() => void createCollection()} disabled={!newCollectionName.trim()} className="flex h-7 w-7 items-center justify-center rounded border border-gray-300 disabled:opacity-40 dark:border-gray-700"><Plus className="h-3.5 w-3.5" /></button></div>
            <div className="mt-1 space-y-0.5">{snapshot.collections.map((collection) => <button key={collection.id} type="button" onClick={() => { setActiveCollectionId(collection.id); setActiveTagId(null); }} className={`flex w-full items-center justify-between gap-2 rounded px-2 py-1.5 text-left text-xs ${activeCollectionId === collection.id ? 'bg-teal-100 text-teal-900 dark:bg-teal-950/55 dark:text-teal-100' : 'hover:bg-gray-200/70 dark:hover:bg-gray-800'}`}><span className="truncate">{collection.name}</span><span className="text-[10px] opacity-65">{collection.itemCount}</span></button>)}</div>
            <div className="mt-5 px-2"><p className="text-[10px] font-medium text-gray-400">标签</p><div className="mt-1 flex flex-wrap gap-1">{snapshot.tags.map((tag) => <button key={tag.id} type="button" onClick={() => { setActiveTagId(tag.id); setActiveCollectionId(null); }} className={`max-w-full rounded border px-1.5 py-0.5 text-[10px] ${activeTagId === tag.id ? 'border-teal-500 bg-teal-100 text-teal-800 dark:bg-teal-950/55 dark:text-teal-200' : 'border-gray-200 text-gray-500 hover:bg-gray-200 dark:border-gray-700 dark:hover:bg-gray-800'}`}>#{tag.name} {tag.itemCount}</button>)}</div></div>
          </> : null}
        </div>
      </aside>

      <main className={`min-w-0 flex-1 overflow-hidden ${showMobileDetail ? 'hidden md:flex' : 'flex'} flex-col`}>
        {!snapshot ? <div className="flex h-full flex-col items-center justify-center gap-3 px-6 text-center"><LibraryBig className="h-10 w-10 text-teal-500" /><h3 className="text-base font-semibold">选择一个媒体资料库</h3><p className="max-w-md text-sm text-gray-500">资料库可以是任意普通目录。Nexora 只会在其中创建 `.pm_center/media_catalog.db` 和可选的 `media/` 归档目录。</p><button type="button" onClick={() => void selectLibraryDirectory()} className="h-9 rounded bg-teal-600 px-3 text-sm font-medium text-white hover:bg-teal-700">选择目录</button></div> : <>
          <header className="flex min-h-[64px] flex-wrap items-center gap-3 border-b border-gray-200 px-4 py-3 dark:border-gray-800"><div className="min-w-0 flex-1"><div className="flex items-center gap-2"><h3 className="truncate text-base font-semibold">{browserTitle}</h3><span className="text-xs text-gray-500">{snapshot.totalItems} 项</span>{snapshot.library.duplicateGroupCount > 0 ? <span className="rounded bg-amber-100 px-1.5 py-0.5 text-[10px] text-amber-800 dark:bg-amber-950/50 dark:text-amber-200">{snapshot.library.duplicateGroupCount} 组重复</span> : null}</div><p className="truncate text-[11px] text-gray-500" title={snapshot.library.rootPath}>{snapshot.library.rootPath}</p></div><div className="flex h-8 items-center rounded border border-gray-300 px-2 dark:border-gray-700"><Search className="mr-1.5 h-3.5 w-3.5 text-gray-400" /><input value={search} onChange={(event) => setSearch(event.target.value)} placeholder="搜索名称或标签" className="w-36 bg-transparent text-xs outline-none" /></div></header>
          <div className="flex shrink-0 flex-wrap items-center gap-2 border-b border-gray-200 px-4 py-2 dark:border-gray-800"><div className="inline-flex rounded border border-gray-300 p-0.5 dark:border-gray-700">{([{ value: 'reference', label: '仅引用' }, { value: 'copy', label: '复制入库' }, { value: 'move', label: '移动归档' }] as const).map((option) => <button key={option.value} type="button" onClick={() => setImportMode(option.value)} className={`h-7 rounded px-2 text-[11px] ${importMode === option.value ? 'bg-teal-600 text-white' : 'text-gray-600 hover:bg-gray-100 dark:text-gray-300 dark:hover:bg-gray-800'}`}>{option.label}</button>)}</div><button type="button" disabled={isImporting} onClick={() => void importMedia(false)} className="flex h-8 items-center gap-1.5 rounded border border-gray-300 px-2.5 text-xs hover:bg-gray-100 disabled:opacity-50 dark:border-gray-700 dark:hover:bg-gray-800">{isImporting ? <LoaderCircle className="h-3.5 w-3.5 animate-spin" /> : <Plus className="h-3.5 w-3.5" />}添加文件</button><button type="button" disabled={isImporting} onClick={() => void importMedia(true)} className="flex h-8 items-center gap-1.5 rounded border border-gray-300 px-2.5 text-xs hover:bg-gray-100 disabled:opacity-50 dark:border-gray-700 dark:hover:bg-gray-800"><FolderOpen className="h-3.5 w-3.5" />添加目录</button></div>
          <div className="min-h-0 flex-1 overflow-auto p-4">{isLoading ? <div className="flex h-full items-center justify-center"><LoaderCircle className="h-5 w-5 animate-spin text-gray-400" /></div> : snapshot.items.length === 0 ? <div className="flex h-full flex-col items-center justify-center gap-2 text-center"><Archive className="h-8 w-8 text-gray-300" /><p className="text-sm text-gray-500">还没有媒体资料</p><p className="text-xs text-gray-400">可添加文件或目录；重复内容会被识别并只关联新位置。</p></div> : <div className="grid grid-cols-[repeat(auto-fill,minmax(150px,1fr))] gap-3">{snapshot.items.map((item) => <MediaCard key={item.id} item={item} selected={selectedId === item.id} onSelect={() => selectItem(item)} />)}</div>}</div>
        </>}
      </main>

      {snapshot ? <aside className={`min-w-0 border-l border-gray-200 bg-gray-50 dark:border-gray-800 dark:bg-gray-900 ${showMobileDetail ? 'flex w-full md:w-80' : 'hidden md:flex md:w-80'} flex-col`}>
        <div className="flex h-12 items-center gap-2 border-b border-gray-200 px-3 dark:border-gray-800"><button type="button" title="返回资料列表" onClick={() => setShowMobileDetail(false)} className="flex h-7 w-7 items-center justify-center rounded hover:bg-gray-200 dark:hover:bg-gray-800 md:hidden"><ChevronLeft className="h-4 w-4" /></button><span className="truncate text-sm font-medium">资料详情</span></div>
        {selected ? <div className="min-h-0 flex-1 overflow-auto p-3"><MediaPreview item={selected} /><h4 className="mt-3 break-words text-sm font-medium">{selected.name}</h4><p className="mt-1 break-all text-[11px] text-gray-500">{selected.primaryPath}</p><div className="mt-3 grid grid-cols-2 gap-px overflow-hidden border border-gray-200 bg-gray-200 text-xs dark:border-gray-800 dark:bg-gray-800"><DetailCell label="类型" value={selected.mediaKind} /><DetailCell label="大小" value={formatBytes(selected.size)} /><DetailCell label="导入" value={formatTime(selected.importedAt)} /><DetailCell label="位置" value={`${selected.locationCount} 个`} /></div>{selected.duplicateCount > 0 ? <p className="mt-3 rounded border border-amber-200 bg-amber-50 px-2 py-1.5 text-[11px] text-amber-800 dark:border-amber-900/70 dark:bg-amber-950/30 dark:text-amber-200">内容与 {selected.duplicateCount} 个其他条目重复</p> : null}<div className="mt-4"><p className="mb-1 text-[11px] font-medium text-gray-500">评分</p><div className="flex gap-1">{[1,2,3,4,5].map((value) => <button key={value} type="button" title={`${value} 星`} onClick={() => void setRating(value)} className={value <= selected.rating ? 'text-amber-500' : 'text-gray-300 dark:text-gray-700'}><Star className="h-4 w-4 fill-current" /></button>)}</div></div><label className="mt-4 block text-[11px] font-medium text-gray-500">备注<textarea value={noteInput} onChange={(event) => setNoteInput(event.target.value)} className="mt-1 h-20 w-full resize-y rounded border border-gray-300 bg-white p-2 text-xs font-normal text-gray-900 outline-none dark:border-gray-700 dark:bg-gray-950 dark:text-gray-100" /></label><label className="mt-3 block text-[11px] font-medium text-gray-500">标签，使用逗号分隔<input value={tagInput} onChange={(event) => setTagInput(event.target.value)} className="mt-1 h-8 w-full rounded border border-gray-300 bg-white px-2 text-xs font-normal text-gray-900 outline-none dark:border-gray-700 dark:bg-gray-950 dark:text-gray-100" /></label><button type="button" onClick={() => void saveMetadata()} className="mt-3 flex h-8 w-full items-center justify-center gap-1.5 rounded bg-gray-900 text-xs font-medium text-white hover:bg-gray-800 dark:bg-white dark:text-gray-900"><Tag className="h-3.5 w-3.5" />保存资料</button></div> : <div className="flex h-full items-center justify-center px-6 text-center text-sm text-gray-500">选择一个资料查看预览、评分和备注</div>}
      </aside> : null}
    </div>
  );
}

function MediaCard({ item, selected, onSelect }: { item: MediaCatalogItem; selected: boolean; onSelect: () => void }) {
  const Icon = kindIcon(item.mediaKind);
  const source = item.mediaKind === 'image' ? previewSource(item) : '';
  return <button type="button" onClick={onSelect} className={`group min-w-0 overflow-hidden border text-left transition-colors ${selected ? 'border-teal-600 ring-1 ring-teal-600' : 'border-gray-200 hover:border-gray-400 dark:border-gray-800 dark:hover:border-gray-600'}`}><div className="relative aspect-[4/3] bg-gray-100 dark:bg-gray-900">{source ? <img src={source} alt="" className="h-full w-full object-cover" loading="lazy" /> : <div className="flex h-full items-center justify-center text-gray-400"><Icon className="h-8 w-8" /></div>}<span className="absolute right-1.5 top-1.5 rounded bg-black/55 px-1.5 py-0.5 text-[10px] text-white">{item.mediaKind}</span></div><div className="min-w-0 p-2"><p className="truncate text-xs font-medium" title={item.name}>{item.name}</p><p className="mt-1 text-[10px] text-gray-500">{formatBytes(item.size)}{item.duplicateCount > 0 ? ` · 重复 ${item.duplicateCount}` : ''}</p>{item.tags.length > 0 ? <p className="mt-1 truncate text-[10px] text-teal-700 dark:text-teal-300">#{item.tags.join(' #')}</p> : null}</div></button>;
}

function MediaPreview({ item }: { item: MediaCatalogItem }) {
  const source = previewSource(item);
  if (item.mediaKind === 'image') return <div className="aspect-video overflow-hidden border border-gray-200 bg-gray-200 dark:border-gray-800 dark:bg-gray-950"><img src={source} alt={item.name} className="h-full w-full object-contain" /></div>;
  if (item.mediaKind === 'video') return <video controls src={source} className="aspect-video w-full border border-gray-200 bg-black dark:border-gray-800" />;
  const Icon = kindIcon(item.mediaKind);
  return <div className="flex aspect-video items-center justify-center border border-gray-200 bg-gray-100 text-gray-400 dark:border-gray-800 dark:bg-gray-950"><Icon className="h-10 w-10" /></div>;
}

function DetailCell({ label, value }: { label: string; value: string }) {
  return <div className="min-w-0 bg-white p-2 dark:bg-gray-900"><p className="text-[10px] text-gray-400">{label}</p><p className="mt-0.5 truncate text-[11px]" title={value}>{value}</p></div>;
}
