import { SettingsShell, type SettingsShellProps } from './settings/SettingsShell'

export type { SettingsTab } from './settings/SettingsShell'
export { SettingsShell } from './settings/SettingsShell'

type SettingsProps = Omit<SettingsShellProps, 'variant'>

/**
 * 独立设置窗（640×520），供翻译器与托盘「设置」使用。
 */
export default function Settings(props: SettingsProps) {
  return <SettingsShell variant="standalone" {...props} />
}
