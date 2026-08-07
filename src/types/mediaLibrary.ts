export type MediaImportMode = 'reference' | 'copy' | 'move';

export interface MediaLibraryBookmark {
  rootPath: string;
  displayName: string;
  lastOpenedAt: number;
  available: boolean;
}

export interface MediaLibraryInfo {
  rootPath: string;
  displayName: string;
  catalogPath: string;
  archivePath: string;
  itemCount: number;
  duplicateGroupCount: number;
  updatedAt: number | null;
}

export interface MediaCatalogItem {
  id: string;
  name: string;
  mediaKind: 'image' | 'video' | 'audio' | 'reference' | string;
  status: string;
  primaryPath: string;
  size: number;
  modifiedAt: number | null;
  importedAt: number;
  updatedAt: number;
  rating: number;
  note: string;
  tags: string[];
  locationCount: number;
  duplicateCount: number;
}

export interface MediaCollection {
  id: string;
  name: string;
  color: string | null;
  itemCount: number;
  createdAt: number;
}

export interface MediaTag {
  id: string;
  name: string;
  color: string | null;
  itemCount: number;
}

export interface MediaLibrarySnapshot {
  library: MediaLibraryInfo;
  items: MediaCatalogItem[];
  collections: MediaCollection[];
  tags: MediaTag[];
  totalItems: number;
  offset: number;
  limit: number;
}

export interface MediaImportItemResult {
  sourcePath: string;
  itemId: string | null;
  outcome: string;
  message: string | null;
}

export interface MediaImportResult {
  imported: number;
  duplicatesLinked: number;
  failed: number;
  items: MediaImportItemResult[];
}
