import type { Components, UrlTransform } from 'react-markdown'
import ReactMarkdown, { defaultUrlTransform } from 'react-markdown'
import remarkGfm from 'remark-gfm'
import remarkMath from 'remark-math'
import rehypeKatex from 'rehype-katex'
import 'katex/dist/katex.min.css'
import { normalizeMarkdownForRender } from './markdownUtils'
import { MarkdownErrorBoundary } from './MarkdownErrorBoundary'
import type { ChatToolArtifact } from './types'

interface ChatMarkdownProps {
  content: string
  artifacts?: ChatToolArtifact[]
}

const proseClass =
  'prose prose-sm dark:prose-invert max-w-none break-words text-[15px] leading-[1.7] text-neutral-900 dark:text-neutral-100 prose-p:my-2 prose-headings:my-3 prose-ul:my-2 prose-ol:my-2 prose-pre:my-2 prose-li:my-0.5 prose-table:my-3 prose-table:shadow-none'

const markdownComponents: Components = {
  table: ({ children }) => (
    <div className="my-3 max-w-full overflow-x-auto">
      <table className="w-full min-w-[240px] border-collapse text-[13px] leading-snug">
        {children}
      </table>
    </div>
  ),
  thead: ({ children }) => (
    <thead className="bg-neutral-50 dark:bg-neutral-800/90">{children}</thead>
  ),
  th: ({ children }) => (
    <th className="border border-neutral-200/90 px-3 py-2 text-left font-semibold text-neutral-800 dark:border-neutral-700 dark:text-neutral-100">
      {children}
    </th>
  ),
  td: ({ children }) => (
    <td className="border border-neutral-200/90 px-3 py-2 align-top text-neutral-700 dark:border-neutral-700 dark:text-neutral-300">
      {children}
    </td>
  ),
}

function artifactDataUrl(artifact: ChatToolArtifact): string {
  return artifact.dataUrl ?? artifact.data_url ?? ''
}

function safeDecodeURIComponent(value: string): string {
  try {
    return decodeURIComponent(value)
  } catch {
    return value
  }
}

function artifactKey(name: string): string {
  return safeDecodeURIComponent(name)
    .trim()
    .replace(/^\.?\//, '')
    .replace(/\\/g, '/')
    .toLowerCase()
}

function artifactBasename(name: string): string {
  return artifactKey(name).split('/').filter(Boolean).pop() ?? artifactKey(name)
}

function isExternalOrAbsoluteImageSrc(src: string): boolean {
  return /^(https?:|data:|blob:|tauri:|asset:|file:|\/)/i.test(src)
}

function isSafeImageDataUrl(src: string): boolean {
  return /^data:image\/(?:png|jpe?g|gif|webp|svg\+xml);base64,[a-z0-9+/=\s]+$/i.test(src.trim())
}

const chatMarkdownUrlTransform: UrlTransform = (url, key, node) => {
  if (key === 'src' && node.tagName === 'img' && isSafeImageDataUrl(url)) {
    return url
  }
  return defaultUrlTransform(url)
}

function buildArtifactLookup(artifacts: ChatToolArtifact[]): Map<string, string> {
  const lookup = new Map<string, string>()
  for (const artifact of artifacts) {
    const dataUrl = artifactDataUrl(artifact)
    if (!artifact.name || !dataUrl.startsWith('data:image/')) continue
    lookup.set(artifactKey(artifact.name), dataUrl)
    lookup.set(artifactBasename(artifact.name), dataUrl)
  }
  return lookup
}

export function ChatMarkdown({ content, artifacts = [] }: ChatMarkdownProps) {
  const normalized = normalizeMarkdownForRender(content)
  const artifactLookup = buildArtifactLookup(artifacts)
  const components: Components = {
    ...markdownComponents,
    img: ({ src, alt }) => {
      const rawSrc = typeof src === 'string' ? src : ''
      const resolvedSrc = rawSrc && !isExternalOrAbsoluteImageSrc(rawSrc)
        ? artifactLookup.get(artifactKey(rawSrc)) ?? artifactLookup.get(artifactBasename(rawSrc)) ?? rawSrc
        : rawSrc
      return (
        <img
          src={resolvedSrc}
          alt={alt ?? ''}
          loading="lazy"
          className="my-3 max-h-[420px] max-w-full rounded-md border border-neutral-200/90 bg-white object-contain dark:border-neutral-700 dark:bg-neutral-900"
        />
      )
    },
  }

  return (
    <div className={proseClass}>
      <MarkdownErrorBoundary fallbackText={content}>
        <ReactMarkdown
          remarkPlugins={[remarkGfm, remarkMath]}
          rehypePlugins={[rehypeKatex]}
          components={components}
          urlTransform={chatMarkdownUrlTransform}
        >
          {normalized}
        </ReactMarkdown>
      </MarkdownErrorBoundary>
    </div>
  )
}
