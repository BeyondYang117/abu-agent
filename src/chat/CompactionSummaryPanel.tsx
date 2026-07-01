import { useState } from 'react'
import { ChevronDown } from 'lucide-react'
import { ChatMarkdown } from './ChatMarkdown'
import { i18n, type Lang } from '../settings/i18n'
import { compactionRecordTokens, type CompactionBoundaryView } from './compactionBoundary'

interface CompactionSummaryPanelProps {
  boundary: CompactionBoundaryView
  lang?: Lang
}

export function CompactionSummaryPanel({ boundary, lang = 'zh' }: CompactionSummaryPanelProps) {
  const t = i18n[lang]
  const [open, setOpen] = useState(false)
  const summary = compactionRecordTokens(boundary.record).summary.trim()
  if (!summary) return null

  return (
    <div className="chat-compaction-summary">
      <button
        type="button"
        className="chat-compaction-summary-toggle"
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
      >
        <ChevronDown
          size={14}
          className={`chat-compaction-summary-chevron ${open ? 'is-open' : ''}`}
          aria-hidden="true"
        />
        <span>{t.contextCompactionSummaryToggle}</span>
      </button>
      <div className={`chat-motion-reveal ${open ? 'is-open' : ''}`} aria-hidden={!open}>
        <div className="chat-compaction-summary-body">
          <ChatMarkdown content={summary} />
        </div>
      </div>
    </div>
  )
}
