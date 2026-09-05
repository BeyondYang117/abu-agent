export const CHAT_DIRECTORY_DRAG_EVENT = 'abu-chat-directory-drag'
export const CHAT_ADD_COLLABORATION_DIRS_EVENT = 'abu-chat-add-collaboration-dirs'

export type DirectoryDragEventDetail =
  | { type: 'start'; path: string; name: string; kind?: 'dir' | 'file' }
  | { type: 'over'; path: string; overComposer: boolean; kind?: 'dir' | 'file' }
  | { type: 'drop'; path: string; overComposer: boolean; kind?: 'dir' | 'file' }
  | { type: 'end' }

export function emitDirectoryDrag(detail: DirectoryDragEventDetail): void {
  window.dispatchEvent(new CustomEvent<DirectoryDragEventDetail>(CHAT_DIRECTORY_DRAG_EVENT, { detail }))
}

export function emitCollaborationDirectories(paths: string[]): void {
  window.dispatchEvent(new CustomEvent<string[]>(CHAT_ADD_COLLABORATION_DIRS_EVENT, { detail: paths }))
}
