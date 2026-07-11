export interface DirEntry {
  name: string;
  path: string;
  is_dir: boolean;
  size: number;
  /** Epoch milliseconds, or null when the filesystem reports none. */
  modified: number | null;
  /** Lowercase extension without the dot; "" for folders/extensionless files. */
  extension: string;
}

export interface Place {
  name: string;
  path: string;
  /** desktop | documents | downloads | pictures | music | videos | drive | home */
  kind: string;
}

export type SortKey = "name" | "modified" | "type" | "size";
export type SortDir = "asc" | "desc";
