import type { ChatToolArtifact } from './types'

export function artifactDataUrl(artifact: ChatToolArtifact): string {
  return artifact.dataUrl ?? artifact.data_url ?? ''
}

export function artifactMimeType(artifact: ChatToolArtifact): string {
  return (artifact.mimeType ?? artifact.mime_type ?? '').toLowerCase()
}

export function isImageArtifact(artifact: ChatToolArtifact): boolean {
  return artifactDataUrl(artifact).startsWith('data:image/') || artifactMimeType(artifact).startsWith('image/')
}

export function isFileArtifact(artifact: ChatToolArtifact): boolean {
  return Boolean(artifact.name) && !isImageArtifact(artifact)
}
