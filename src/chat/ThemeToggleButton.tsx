import { Moon, Sun } from 'lucide-react'
import { useT, type I18n } from '../settings/i18n'
import { chatTitlebarIconButtonClass } from './platform'
import { resolveThemeIconMode, type ThemeMode } from './themeMode'

function modeLabel(mode: ThemeMode, t: I18n): string {
  if (mode === 'light') return t.themeLight
  if (mode === 'dark') return t.themeDark
  return t.themeSystem
}

export function ThemeToggleButton({ themeMode, onToggle }: { themeMode: ThemeMode; onToggle: () => void }) {
  const t = useT()
  const iconMode = resolveThemeIconMode(themeMode)
  const label = `${t.theme}: ${modeLabel(themeMode, t)}`
  const Icon = iconMode === 'dark' ? Moon : Sun
  return (
    <button
      type="button"
      className={chatTitlebarIconButtonClass}
      onClick={onToggle}
      title={label}
      aria-label={label}
      data-tauri-drag-region="false"
    >
      <Icon size={15} strokeWidth={1.75} />
    </button>
  )
}
