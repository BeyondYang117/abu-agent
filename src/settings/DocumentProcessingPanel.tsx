// 文档处理设置区（知识库页）：仅 Kivio 内置本地解析 + 图片 OCR。
// 第三方处理器（MinerU/Doc2X/自定义）已挂起。
import { Download, Info, RefreshCw } from 'lucide-react'
import { useEffect, useState } from 'react'
import {
  api,
  type DocumentProcessingConfig,
  type OcrEngine,
  type PdfStrategy,
  type RapidOcrStatus,
} from '../api/tauri'
import { type Lang } from './i18n'
import { SettingsGroup, Select } from './components'

const EMPTY: DocumentProcessingConfig = {
  ocrEngine: 'off',
  pdfStrategy: 'text',
}

export function DocumentProcessingPanel({
  config,
  lang,
  onChange,
}: {
  config?: DocumentProcessingConfig
  lang: Lang
  onChange: (next: DocumentProcessingConfig) => void
}) {
  const t = (zh: string, en: string) => (lang === 'zh' ? zh : en)
  const cfg = config ?? EMPTY

  const patch = (updates: Partial<DocumentProcessingConfig>) => onChange({ ...cfg, ...updates })

  const isMac = typeof navigator !== 'undefined' && /Mac/i.test(navigator.userAgent)

  return (
    <SettingsGroup title={t('文档处理', 'Document processing')}>
      <p className="flex items-start gap-1.5 px-1 py-1 text-xs text-zinc-500">
        <Info size={13} className="mt-0.5 shrink-0" />
        <span>
          {t(
            '用 Kivio 本地解析（txt/md/html、PDF 文字层、docx/xlsx），免费、离线、批量友好。图片可选用 OCR 引擎识别文字后入库。',
            'Uses Kivio local parsing (txt/md/html, PDF text layer, docx/xlsx) — free, offline, batch-friendly. Images can be OCR’d into text via the selected engine.',
          )}
        </span>
      </p>

      {/* OCR 引擎 */}
      <div className="flex flex-wrap items-center gap-2 py-2">
        <span className="text-sm text-zinc-500">{t('OCR 引擎', 'OCR engine')}</span>
        <Select
          className="w-56"
          value={cfg.ocrEngine}
          onChange={(v) => patch({ ocrEngine: v as OcrEngine })}
          options={[
            { value: 'off', label: t('关闭', 'Off') },
            { value: 'system', label: t('系统 OCR', 'System OCR') },
            { value: 'rapid_ocr', label: t('RapidOCR 离线', 'RapidOCR (offline)') },
          ]}
        />
      </div>

      {cfg.ocrEngine === 'system' && (
        <div className="kv-panel mt-1">
          <div className="kv-panel-body">
            {isMac
              ? t('macOS：Apple Vision。', 'macOS: Apple Vision.')
              : t(
                  'Windows：Windows.Media.Ocr；其他平台不可用。',
                  'Windows: Windows.Media.Ocr; unavailable on other platforms.',
                )}
          </div>
        </div>
      )}

      {cfg.ocrEngine === 'rapid_ocr' && <RapidOcrWidget t={t} />}

      {/* PDF 处理策略 */}
      <div className="flex flex-wrap items-center gap-2 py-2">
        <span className="text-sm text-zinc-500">{t('PDF 处理策略', 'PDF strategy')}</span>
        <Select
          className="w-56"
          value={cfg.pdfStrategy}
          onChange={(v) => patch({ pdfStrategy: v as PdfStrategy })}
          options={[
            { value: 'text', label: t('文字层优先', 'Text layer') },
            { value: 'force_ocr', label: t('强制 OCR', 'Force OCR') },
          ]}
        />
      </div>

      {cfg.pdfStrategy === 'force_ocr' && (
        <p className="flex items-start gap-1.5 px-1 text-[11px] text-amber-700 dark:text-amber-200">
          <Info size={12} className="mt-0.5 shrink-0" />
          <span>
            {t(
              '内置仅支持 PDF 文字层，强制 OCR（扫描版）暂未启用。',
              'Built-in only supports the PDF text layer; force OCR (scanned PDFs) is not yet enabled.',
            )}
          </span>
        </p>
      )}

      {/* 支持格式一览 */}
      <div className="kv-panel mt-2">
        <div className="kv-panel-title !mb-0">{t('支持格式', 'Supported formats')}</div>
        <div className="kv-panel-body">
          {t(
            'txt / md / html / PDF（文字层）/ docx / xlsx；图片（png/jpg/webp 等，需开启 OCR）。',
            'txt / md / html / PDF (text layer) / docx / xlsx; images (png/jpg/webp, requires OCR).',
          )}
        </div>
      </div>
    </SettingsGroup>
  )
}

/** RapidOCR 离线引擎的状态/下载组件，本地自管状态。 */
function RapidOcrWidget({ t }: { t: (zh: string, en: string) => string }) {
  const [status, setStatus] = useState<RapidOcrStatus | null>(null)
  const [downloadState, setDownloadState] = useState<'idle' | 'downloading' | 'failed'>('idle')
  const [downloadError, setDownloadError] = useState('')

  const refresh = () => {
    api
      .rapidOcrStatus()
      .then(setStatus)
      .catch(() => setStatus({ modelsAvailable: false }))
  }

  useEffect(() => {
    refresh()
  }, [])

  const download = async () => {
    setDownloadState('downloading')
    setDownloadError('')
    try {
      const res = await api.rapidOcrInstall()
      if (res.success) {
        setDownloadState('idle')
        refresh()
      } else {
        setDownloadState('failed')
        setDownloadError(res.message)
      }
    } catch (e) {
      setDownloadState('failed')
      setDownloadError(String(e))
    }
  }

  return (
    <div className="kv-panel mt-1">
      {status?.modelsAvailable ? (
        <div className="flex items-start gap-2">
          <span className="mt-0.5 inline-block h-1.5 w-1.5 rounded-full bg-emerald-500" />
          <div className="flex-1">
            <div className="kv-panel-title !mb-0">{t('RapidOCR 已就绪', 'RapidOCR ready')}</div>
            {status.modelDir && (
              <div className="kv-panel-body break-all font-mono">{status.modelDir}</div>
            )}
          </div>
          <button onClick={refresh} className="kv-icon-btn" title={t('刷新', 'Refresh')}>
            <RefreshCw size={12} strokeWidth={2.25} />
          </button>
        </div>
      ) : (
        <div className="space-y-2.5">
          <div className="flex items-start gap-2">
            <span className="mt-0.5 inline-block h-1.5 w-1.5 rounded-full bg-amber-500" />
            <div className="kv-panel-title !mb-0 flex-1">
              {t('RapidOCR 模型未下载', 'RapidOCR models not downloaded')}
            </div>
            <button
              onClick={refresh}
              disabled={downloadState === 'downloading'}
              className="kv-icon-btn disabled:opacity-40"
              title={t('刷新', 'Refresh')}
            >
              <RefreshCw size={12} strokeWidth={2.25} />
            </button>
          </div>

          {downloadState === 'downloading' ? (
            <div className="kv-panel-body flex items-center gap-2 pl-3.5">
              <RefreshCw size={12} strokeWidth={2.25} className="animate-spin" />
              <span>{t('正在下载…', 'Downloading…')}</span>
            </div>
          ) : (
            <div className="pl-3.5">
              <button onClick={download} className="kv-btn primary">
                <Download size={12} strokeWidth={2.5} />
                {t('下载离线模型', 'Download models')}
              </button>
            </div>
          )}

          {downloadState === 'failed' && downloadError && (
            <div className="kv-inline-error break-words pl-3.5">
              {t('下载失败', 'Download failed')}: {downloadError}
            </div>
          )}
        </div>
      )}
    </div>
  )
}
