export interface DirEntry {
  name: string;
  path: string;
  is_dir: boolean;
  size: number;
  /** Epoch milliseconds, or null when the filesystem reports none. */
  modified: number | null;
  /** Lowercase extension without the dot; "" for folders/extensionless files. */
  extension: string;
  /** Hidden per OS convention: hidden attribute on Windows, dotfile on POSIX. */
  hidden: boolean;
}

export interface Place {
  name: string;
  path: string;
  /** desktop | documents | downloads | pictures | music | videos | drive | home */
  kind: string;
}

export type SortKey = "name" | "modified" | "type" | "size";
export type SortDir = "asc" | "desc";
export type ViewMode = "details" | "list" | "icons";

export interface RecentFile {
  path: string;
  name: string;
  /** Epoch ms when it was last opened from this app. */
  opened: number;
}

export interface Favorite {
  path: string;
  name: string;
  /** Folders navigate on click; files open. */
  is_dir: boolean;
}
