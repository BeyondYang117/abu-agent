// 连接器品牌图标：单色线性/实心品牌 logo，跟随文字色（currentColor），深色模式自动反色。
// 品牌 path 来源：
//   - Notion：simple-icons 官方 SVG（https://cdn.simpleicons.org/notion，path 内联）。
//   - GitHub：直接复用 lucide-react 的 Github 图标（仓库已依赖 lucide-react，准确且省事）。
//   - Composio：simple-icons 无该 slug（抓取返回空），退而用字母标记（圆角方块 + 首字母「C」）。
//   - 自定义连接器：通用 link 图标（fallback）。

import { Github, Link2 } from 'lucide-react'

type BrandIconProps = { size?: number; className?: string }

// Notion：simple-icons 官方品牌 path（fill currentColor）。
export function NotionBrandIcon({ size = 22, className }: BrandIconProps) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="currentColor"
      role="img"
      aria-label="Notion"
      className={className}
    >
      <path d="M4.459 4.208c.746.606 1.026.56 2.428.466l13.215-.793c.28 0 .047-.28-.046-.326L17.86 1.968c-.42-.326-.981-.7-2.055-.607L3.01 2.295c-.466.046-.56.28-.374.466zm.793 3.08v13.904c0 .747.373 1.027 1.214.98l14.523-.84c.841-.046.935-.56.935-1.167V6.354c0-.606-.233-.933-.748-.887l-15.177.887c-.56.047-.747.327-.747.933zm14.337.745c.093.42 0 .84-.42.888l-.7.14v10.264c-.608.327-1.168.514-1.635.514-.748 0-.935-.234-1.495-.933l-4.577-7.186v6.952L12.21 19s0 .84-1.168.84l-3.222.186c-.093-.186 0-.653.327-.746l.84-.233V9.854L7.822 9.76c-.094-.42.14-1.026.793-1.073l3.456-.233 4.764 7.279v-6.44l-1.215-.139c-.093-.514.28-.887.747-.933zM1.936 1.035l13.31-.98c1.634-.14 2.055-.047 3.082.7l4.249 2.986c.7.513.934.653.934 1.213v16.378c0 1.026-.373 1.634-1.68 1.726l-15.458.934c-.98.047-1.448-.093-1.962-.747l-3.129-4.06c-.56-.747-.793-1.306-.793-1.96V2.667c0-.839.374-1.54 1.447-1.632z" />
    </svg>
  )
}

// GitHub：复用 lucide-react 的 Github 图标（线性单色，跟随文字色）。
export function GithubBrandIcon({ size = 22, className }: BrandIconProps) {
  return <Github size={size} className={className} />
}

// Composio：simple-icons 无对应 slug，退而用首字母标记（圆角方块 + 「C」）。
export function ComposioBrandIcon({ size = 22, className }: BrandIconProps) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      role="img"
      aria-label="Composio"
      className={className}
    >
      <rect x="2" y="2" width="20" height="20" rx="5" fill="currentColor" />
      <text
        x="12"
        y="16.5"
        textAnchor="middle"
        fontSize="13"
        fontWeight="700"
        fontFamily="ui-sans-serif, system-ui, sans-serif"
        className="fill-white dark:fill-black"
      >
        C
      </text>
    </svg>
  )
}

// 自定义连接器 / 未知：通用 link 图标。
export function CustomConnectorIcon({ size = 22, className }: BrandIconProps) {
  return <Link2 size={size} className={className} />
}
