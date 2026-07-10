import type { ChatToolArtifact } from './types'

export function artifactDataUrl(artifact: ChatToolArtifact): string {
  return artifact.dataUrl ?? artifact.data_url ?? ''
}

export function artifactMimeType(artifact: ChatToolArtifact): string {
  return (artifact.mimeType ?? artifact.mime_type ?? '').toLowerCase()
}

export function isImageArtifact(artifact: ChatToolArtifact): boolean {
  // 外置后可能只剩 path + 空/缩略 data_url；mime 或 path 扩展名也算图
  if (artifactDataUrl(artifact).startsWith('data:image/')) return true
  if (artifactMimeType(artifact).startsWith('image/')) return true
  const path = (artifact.path ?? '').toLowerCase()
  return /\.(png|jpe?g|gif|webp|svg)$/i.test(path)
}

export function isFileArtifact(artifact: ChatToolArtifact): boolean {
  return Boolean(artifact.name) && !isImageArtifact(artifact)
}
