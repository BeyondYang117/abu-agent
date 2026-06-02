export const isMac =
  typeof navigator !== 'undefined' && /Mac|iPhone|iPad|iPod/i.test(navigator.userAgent)

/** macOS Chat 使用 Tauri Overlay 标题栏，交通灯由系统绘制 */
export const usesNativeTitlebar = isMac

/** 侧栏 / 主内容顶栏行高与垂直居中 */
export const chatTitlebarRowClass = usesNativeTitlebar
  ? 'flex h-[52px] shrink-0 items-center gap-2'
  : 'flex h-[52px] shrink-0 items-center gap-2 px-3 pt-2'

/** 窗口左缘交通灯留白（仅侧栏顶栏、收起态主顶栏） */
export const chatTitlebarMacInsetClass = usesNativeTitlebar ? 'pl-[76px]' : ''

/** 顶栏内模型选择与图标按钮视觉对齐 */
export const chatTitlebarModelClass = 'mt-1.5'
