import { RefreshCw } from 'lucide-react'
import { Button } from '../components/Button'
import { TextArea } from './components'

/** Shared editable prompt field used by Lens and Chat settings. */
export function PromptField({
  label,
  description,
  value,
  defaultText,
  restoreLabel,
  onChange,
}: {
  label: string
  description?: string
  value: string
  defaultText: string
  restoreLabel: string
  onChange: (value: string) => void
}) {
  return (
    <div className="py-2">
      <div className="mb-2 flex items-start justify-between gap-2">
        <div>
          <div className="kv-row-label">{label}</div>
          {description && <p className="kv-row-desc">{description}</p>}
        </div>
        <Button
          size="sm"
          className="shrink-0"
          onClick={() => onChange('')}
          disabled={!defaultText && !value}
          data-tauri-drag-region="false"
        >
          <RefreshCw size={10} />
          {restoreLabel}
        </Button>
      </div>
      <TextArea value={value || defaultText} onChange={onChange} rows={4} />
    </div>
  )
}
