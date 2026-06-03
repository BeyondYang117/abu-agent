import ReactMarkdown from 'react-markdown'
import remarkMath from 'remark-math'
import rehypeKatex from 'rehype-katex'
import 'katex/dist/katex.min.css'

interface ChatMarkdownProps {
  content: string
}

const proseClass =
  'prose prose-sm dark:prose-invert max-w-none break-words text-[15px] leading-[1.7] text-neutral-900 dark:text-neutral-100 prose-p:my-2 prose-headings:my-3 prose-ul:my-2 prose-ol:my-2 prose-pre:my-2 prose-li:my-0.5'

export function ChatMarkdown({ content }: ChatMarkdownProps) {
  return (
    <div className={proseClass}>
      <ReactMarkdown remarkPlugins={[remarkMath]} rehypePlugins={[rehypeKatex]}>
        {content}
      </ReactMarkdown>
    </div>
  )
}
