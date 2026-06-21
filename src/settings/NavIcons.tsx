// 设置导航的自定义线性图标。接口对齐 lucide-react（size / strokeWidth / className），
// 可直接替换 <Icon size={17} strokeWidth={1.75} />。fill=none + stroke=currentColor，
// 颜色随上层文字色（选中态 / 深色模式自动跟随）。

interface IconProps {
  size?: number
  strokeWidth?: number
  className?: string
}

function svgProps({ size = 24, strokeWidth = 2, className }: IconProps) {
  return {
    width: size,
    height: size,
    viewBox: '0 0 24 24',
    fill: 'none',
    stroke: 'currentColor',
    strokeWidth,
    strokeLinecap: 'round' as const,
    strokeLinejoin: 'round' as const,
    className,
  }
}

// 基础：六角螺母 + 中心孔
export function GeneralIcon(props: IconProps) {
  return (
    <svg {...svgProps(props)}>
      <path d="M21 12 L16.5 4.2 L7.5 4.2 L3 12 L7.5 19.8 L16.5 19.8 Z" />
      <circle cx="12" cy="12" r="3.2" />
    </svg>
  )
}

// 输入翻译：A 字 + CJK 字符
export function TranslateIcon(props: IconProps) {
  return (
    <svg {...svgProps(props)}>
      <path d="M3 19 L6.5 8 L10 19" />
      <path d="M4.2 15.5 H8.8" />
      <path d="M13 8 H21" />
      <path d="M17 6 V8" />
      <path d="M14.5 18 L17 12.5 L19.5 18" />
    </svg>
  )
}

// 快速翻译：闪电
export function ScreenshotIcon(props: IconProps) {
  return (
    <svg {...svgProps(props)}>
      <path d="M13 2 L4 14 H11 L11 22 L20 10 H13 Z" />
    </svg>
  )
}

// Lens：取景框 + 镜头
export function LensIcon(props: IconProps) {
  return (
    <svg {...svgProps(props)}>
      <circle cx="12" cy="12" r="3.5" />
      <path d="M5 8 V5 H8" />
      <path d="M16 5 H19 V8" />
      <path d="M19 16 V19 H16" />
      <path d="M8 19 H5 V16" />
    </svg>
  )
}

// AI 客户端：对话气泡
export function ChatIcon(props: IconProps) {
  return (
    <svg {...svgProps(props)}>
      <path d="M5 5 H19 A2 2 0 0 1 21 7 V15 A2 2 0 0 1 19 17 H11 L6.5 20.5 V17 H5 A2 2 0 0 1 3 15 V7 A2 2 0 0 1 5 5 Z" />
    </svg>
  )
}

// 记忆：节点图
export function MemoryIcon(props: IconProps) {
  return (
    <svg {...svgProps(props)}>
      <circle cx="7" cy="8" r="2" />
      <circle cx="17" cy="9" r="2" />
      <circle cx="11" cy="16.5" r="2" />
      <path d="M8.9 8.6 L15.1 8.9" />
      <path d="M8.2 9.7 L9.9 14.8" />
      <path d="M15.5 10.6 L12.3 14.9" />
    </svg>
  )
}

// 混音器：推子
export function MixerIcon(props: IconProps) {
  return (
    <svg {...svgProps(props)}>
      <path d="M7 4 V20" />
      <path d="M12 4 V20" />
      <path d="M17 4 V20" />
      <circle cx="7" cy="9" r="1.8" />
      <circle cx="12" cy="14" r="1.8" />
      <circle cx="17" cy="8" r="1.8" />
    </svg>
  )
}

// Kivio Code：终端
export function CodeIcon(props: IconProps) {
  return (
    <svg {...svgProps(props)}>
      <path d="M5 5 H19 A2 2 0 0 1 21 7 V17 A2 2 0 0 1 19 19 H5 A2 2 0 0 1 3 17 V7 A2 2 0 0 1 5 5 Z" />
      <path d="M7 10 L10 12.5 L7 15" />
      <path d="M12.5 15 H16" />
    </svg>
  )
}

// 本地 CLI Agent：机器人
export function AgentIcon(props: IconProps) {
  return (
    <svg {...svgProps(props)}>
      <path d="M7 9 H17 A1.5 1.5 0 0 1 18.5 10.5 V16.5 A1.5 1.5 0 0 1 17 18 H7 A1.5 1.5 0 0 1 5.5 16.5 V10.5 A1.5 1.5 0 0 1 7 9 Z" />
      <path d="M12 9 V6" />
      <circle cx="12" cy="5" r="1" />
      <path d="M9.5 13 V13" />
      <path d="M14.5 13 V13" />
    </svg>
  )
}

// MCP：插头/连接
export function McpIcon(props: IconProps) {
  return (
    <svg {...svgProps(props)}>
      <path d="M9 3 V7" />
      <path d="M15 3 V7" />
      <path d="M7 7 H17 V11 A5 5 0 0 1 7 11 Z" />
      <path d="M12 16 V21" />
    </svg>
  )
}

// Skill：星花
export function SkillIcon(props: IconProps) {
  return (
    <svg {...svgProps(props)}>
      <path d="M12 3 L13.4 10.6 L21 12 L13.4 13.4 L12 21 L10.6 13.4 L3 12 L10.6 10.6 Z" />
    </svg>
  )
}

// 网络搜索：地球
export function WebSearchIcon(props: IconProps) {
  return (
    <svg {...svgProps(props)}>
      <circle cx="12" cy="12" r="9" />
      <path d="M3 12 H21" />
      <path d="M12 3 A4 9 0 0 0 12 21 A4 9 0 0 0 12 3 Z" />
    </svg>
  )
}

// 用量统计：柱状图
export function UsageIcon(props: IconProps) {
  return (
    <svg {...svgProps(props)}>
      <path d="M4 20 H20" />
      <path d="M7.5 20 V13" />
      <path d="M12 20 V7" />
      <path d="M16.5 20 V11" />
    </svg>
  )
}

// 模型：云
export function ProvidersIcon(props: IconProps) {
  return (
    <svg {...svgProps(props)}>
      <path d="M6.5 18 A3.5 3.5 0 0 1 6.8 11 A5 5 0 0 1 16.5 10.5 A3.5 3.5 0 0 1 17 18 Z" />
    </svg>
  )
}

// 关于：信息
export function AboutIcon(props: IconProps) {
  return (
    <svg {...svgProps(props)}>
      <circle cx="12" cy="12" r="9" />
      <path d="M12 11 V16.5" />
      <path d="M12 7.8 V7.81" />
    </svg>
  )
}
