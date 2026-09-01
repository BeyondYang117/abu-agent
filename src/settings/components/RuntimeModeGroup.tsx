import { AlertCircle, Cloud, HardDrive } from 'lucide-react'
import { SettingsGroup, SettingRow } from '../components'
import type { Settings as SettingsData } from '../../api/tauri'

export function RuntimeModeGroup({
  settings,
  lang,
  onUpdateSettings,
}: {
  settings: SettingsData
  lang: 'zh' | 'en'
  onUpdateSettings: (updates: Partial<SettingsData>) => void
}) {
  const currentMode = settings.runtime_mode || 'local'

  const handleModeChange = (mode: 'cloud' | 'local') => {
    if (mode === currentMode) return

    // 提示用户切换模式的影响
    const message = lang === 'zh'
      ? mode === 'cloud'
        ? '切换到云端模式将使用 ABU API 进行模型调用。请确保已登录。'
        : '切换到本地模式将使用本地配置的 API Key。请确保已配置模型提供商。'
      : mode === 'cloud'
        ? 'Switching to Cloud mode will use ABU API for model calls. Please ensure you are logged in.'
        : 'Switching to Local mode will use locally configured API keys. Please ensure providers are configured.'

    if (confirm(message)) {
      onUpdateSettings({ runtime_mode: mode })
    }
  }

  return (
    <SettingsGroup
      title={lang === 'zh' ? '运行模式' : 'Runtime Mode'}
      description={lang === 'zh' ? '选择模型调用方式' : 'Choose how to call models'}
    >
      <div className="space-y-3">
        {/* 提示信息 */}
        <div className="flex items-start gap-2 p-3 rounded-lg bg-blue-50 dark:bg-blue-900/20 border border-blue-200 dark:border-blue-800">
          <AlertCircle size={16} className="text-blue-600 dark:text-blue-400 mt-0.5 flex-shrink-0" />
          <div className="text-xs text-blue-700 dark:text-blue-300">
            {lang === 'zh'
              ? '切换运行模式后，聊天数据将使用新的模式保存，两种模式的数据不互通。'
              : 'After switching modes, chat data will be saved using the new mode. Data between modes is not shared.'
            }
          </div>
        </div>

        {/* 模式选择 */}
        <div className="space-y-2">
          {/* Cloud 模式 */}
          <button
            type="button"
            onClick={() => handleModeChange('cloud')}
            className={`
              w-full p-4 rounded-lg border-2 text-left transition-all
              ${currentMode === 'cloud'
                ? 'border-blue-500 dark:border-blue-400 bg-blue-50 dark:bg-blue-900/20'
                : 'border-neutral-200 dark:border-neutral-700 hover:border-neutral-300 dark:hover:border-neutral-600'
              }
            `}
          >
            <div className="flex items-start gap-3">
              <Cloud
                size={20}
                className={currentMode === 'cloud'
                  ? 'text-blue-600 dark:text-blue-400'
                  : 'text-neutral-500 dark:text-neutral-400'
                }
              />
              <div className="flex-1">
                <div className="font-medium text-neutral-900 dark:text-neutral-100 mb-1">
                  {lang === 'zh' ? '云端模式' : 'Cloud Mode'}
                </div>
                <div className="text-xs text-neutral-600 dark:text-neutral-400">
                  {lang === 'zh'
                    ? '使用 ABU API 进行模型调用，无需配置 API Key，支持统一计费和使用统计。'
                    : 'Use ABU API for model calls. No API key configuration needed. Supports unified billing and usage statistics.'
                  }
                </div>
              </div>
            </div>
          </button>

          {/* Local 模式 */}
          <button
            type="button"
            onClick={() => handleModeChange('local')}
            className={`
              w-full p-4 rounded-lg border-2 text-left transition-all
              ${currentMode === 'local'
                ? 'border-blue-500 dark:border-blue-400 bg-blue-50 dark:bg-blue-900/20'
                : 'border-neutral-200 dark:border-neutral-700 hover:border-neutral-300 dark:hover:border-neutral-600'
              }
            `}
          >
            <div className="flex items-start gap-3">
              <HardDrive
                size={20}
                className={currentMode === 'local'
                  ? 'text-blue-600 dark:text-blue-400'
                  : 'text-neutral-500 dark:text-neutral-400'
                }
              />
              <div className="flex-1">
                <div className="font-medium text-neutral-900 dark:text-neutral-100 mb-1">
                  {lang === 'zh' ? '本地模式' : 'Local Mode'}
                </div>
                <div className="text-xs text-neutral-600 dark:text-neutral-400">
                  {lang === 'zh'
                    ? '使用本地配置的 API Key 直接调用模型提供商，适合企业内网或已有 API Key 的用户。'
                    : 'Use locally configured API keys to call model providers directly. Suitable for enterprise networks or users with existing API keys.'
                  }
                </div>
              </div>
            </div>
          </button>
        </div>
      </div>
    </SettingsGroup>
  )
}
