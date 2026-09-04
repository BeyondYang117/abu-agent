import { RefreshCw, FolderOpen } from 'lucide-react'
import { open } from '@tauri-apps/plugin-dialog'
import { homeDir, join } from '@tauri-apps/api/path'
import { Select, Input, SettingRow, SettingsGroup } from '../components'
import { Button } from '../../components/Button'
import { PromptField } from '../ScreenshotTranslationSettings'
import type { I18n, Lang } from '../i18n'
import type { SettingsTab } from '../SettingsShell'
import { AvatarField } from './AvatarField'
import { ChatToolsStatusGroup } from './ChatToolsStatusGroup'
import type {
  Settings as SettingsData,
  ChatToolsConfig,
  ChatMemoryConfig,
} from '../../api/tauri'

/** 对齐 Pi：自定义模型未填 maxTokens 时的缺省，也是唯一的协议兜底。 */
const CHAT_FALLBACK_MAX_OUTPUT_TOKENS = 16384

function formatTokenCount(tokens?: number): string {
  if (!tokens || !Number.isFinite(tokens)) return ''
  return `${tokens.toLocaleString()} tokens`
}

interface ChatTabProps {
  settings: SettingsData
  t: I18n
  lang: Lang
  chatConfig: NonNullable<SettingsData['chat']>
  chatTools: ChatToolsConfig
  chatMemory: ChatMemoryConfig
  /** Built-in Agent system prompt (exact string used when systemPrompt is empty). */
  chatDefaults: string | undefined
  effectiveChatMaxOutput: { maxOutput: number; source: string }
  chatMaxOutputSourceLabel: string
  chatMaxOutputModelLabel: string
  skillRuntimeEnabled: boolean
  nativeBuiltinToolsEnabled: boolean
  onUpdateChat: (updates: Partial<NonNullable<SettingsData['chat']>>) => void
  onUpdateNativeTools: (updates: Partial<NonNullable<ChatToolsConfig['nativeTools']>>) => void
  onNavigateTab: (tab: SettingsTab) => void
}

/** AI client settings for the visible ABU Agent runtime. */
export function ChatTab({
  settings,
  t,
  lang,
  chatConfig,
  chatTools,
  chatMemory,
  chatDefaults,
  effectiveChatMaxOutput,
  chatMaxOutputSourceLabel,
  chatMaxOutputModelLabel,
  skillRuntimeEnabled,
  nativeBuiltinToolsEnabled,
  onUpdateChat,
  onUpdateNativeTools,
  onNavigateTab,
}: ChatTabProps) {
  return (
    <>
      <SettingsGroup title={lang === 'zh' ? '个人资料' : 'Profile'}>
        <div className="flex items-center gap-3 py-1">
          <AvatarField
            value={chatConfig.userAvatar || ''}
            onChange={(userAvatar) => onUpdateChat({ userAvatar })}
            zh={lang === 'zh'}
          />
          <Input
            className="min-w-0 flex-1"
            value={chatConfig.userDisplayName || ''}
            onChange={(userDisplayName) => onUpdateChat({ userDisplayName })}
            placeholder={lang === 'zh' ? '用户名（选填）' : 'Display name (optional)'}
          />
        </div>
      </SettingsGroup>

      {/* ─── ABU Agent Agent ─── */}
      <SettingsGroup title={t.abuAgentSection}>
        {t.abuAgentSectionHint ? (
          <p className="kv-row-desc mb-2 px-0">{t.abuAgentSectionHint}</p>
        ) : null}

        <SettingRow label={lang === 'zh' ? '普通对话工作目录' : 'Conversation workspace'} stack>
          <div className="flex w-full flex-col gap-2">
            <div className="flex gap-2">
              <Input
                className="min-w-0 flex-1"
                value={chatTools.nativeTools?.workingDirectory ?? ''}
                placeholder={lang === 'zh' ? '默认：~/ABU Agent/workspace' : 'Default: ~/ABU Agent/workspace'}
                onChange={(workingDirectory) => onUpdateNativeTools({ workingDirectory })}
              />
              <Button
                size="sm"
                className="shrink-0"
                onClick={async () => {
                  const selected = await open({ directory: true, multiple: false })
                  if (!selected || typeof selected !== 'string') return
                  onUpdateNativeTools({ workingDirectory: selected })
                }}
                data-tauri-drag-region="false"
              >
                <FolderOpen size={11} />
                {lang === 'zh' ? '选择' : 'Choose'}
              </Button>
              <Button
                size="sm"
                className="shrink-0"
                onClick={async () => {
                  const defaultPath = await join(await homeDir(), 'ABU Agent', 'workspace')
                  onUpdateNativeTools({ workingDirectory: defaultPath })
                }}
                data-tauri-drag-region="false"
              >
                <RefreshCw size={11} />
                {lang === 'zh' ? '恢复默认' : 'Reset'}
              </Button>
            </div>
            <p className="kv-row-desc">
              {lang === 'zh'
                ? '未绑定项目的 Agent 对话会在此目录下按对话 ID 使用独立工作台；用户明确指定的其他路径不受限制。'
                : 'Agent chats get a per-conversation workbench here. Explicit paths chosen by the user remain unrestricted.'}
            </p>
          </div>
        </SettingRow>

        <SettingRow label={t.chatMaxOutputTokens} stack>
          <div className="flex flex-col gap-2 sm:flex-row sm:items-start sm:justify-between">
            <div className="min-w-0">
              <div className="flex flex-wrap items-center gap-2">
                <span className="text-[15px] font-medium text-neutral-900 dark:text-neutral-50">
                  {formatTokenCount(effectiveChatMaxOutput.maxOutput)}
                </span>
                <span className={`kv-tag ${effectiveChatMaxOutput.source === 'fallback' ? 'warn' : 'ok'}`}>
                  {chatMaxOutputSourceLabel}
                </span>
              </div>
              <p className="kv-row-desc mt-1 min-w-0 break-all">
                {lang === 'zh' ? '聊天所选模型：' : 'Chat model: '}
                {chatMaxOutputModelLabel}
              </p>
            </div>
            <div className="flex shrink-0 items-center gap-2">
              <span className="kv-row-desc whitespace-nowrap">
                {lang === 'zh' ? '兜底' : 'Fallback'}
              </span>
              <span className="text-[13px] tabular-nums text-neutral-800 dark:text-neutral-200">
                {formatTokenCount(CHAT_FALLBACK_MAX_OUTPUT_TOKENS)}
              </span>
            </div>
          </div>
        </SettingRow>

        <SettingRow label={t.chatDefaultLanguage}>
          <Select
            className="w-44"
            value={chatConfig.defaultLanguage || ''}
            onChange={(defaultLanguage) => onUpdateChat({ defaultLanguage })}
            options={[
              { value: '', label: t.lensLanguageInherit },
              { value: 'zh', label: '中文' },
              { value: 'zh-Hant', label: '繁體中文' },
              { value: 'en', label: 'English' },
            ]}
          />
        </SettingRow>

        <PromptField
          label={t.abuAgentChatSystemPrompt}
          description={t.chatSystemPromptHint}
          value={chatConfig.systemPrompt || ''}
          defaultText={chatDefaults || ''}
          restoreLabel={t.restoreDefaultPrompt}
          onChange={(systemPrompt) => onUpdateChat({ systemPrompt })}
        />
      </SettingsGroup>

      <ChatToolsStatusGroup
        settings={settings}
        t={t}
        lang={lang}
        chatTools={chatTools}
        chatMemory={chatMemory}
        skillRuntimeEnabled={skillRuntimeEnabled}
        nativeBuiltinToolsEnabled={nativeBuiltinToolsEnabled}
        onNavigateTab={onNavigateTab}
      />

    </>
  )
}
