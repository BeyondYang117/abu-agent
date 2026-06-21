// 连接器逐工具「允许/停用」开关的纯函数：把 ChatMcpServer.enabled_tools 白名单
// 的语义集中在一处，方便单测。
//
// 语义：
//   - enabledTools 为空数组 = 全部工具允许（不过滤）。
//   - 非空 = 仅白名单内的工具允许，其余停用。
//
// 操作：
//   - disable：把某工具从「允许」变「停用」。若当前为空（全允许），先展开成全部
//     工具名再移除该工具；否则直接移除。
//   - allow：把某工具从「停用」变「允许」。加入白名单；若加完恰好等于全部工具，
//     重置为空数组（回到「全部允许」的规范态）。

/** 某工具在当前白名单下是否处于「允许」态。空白名单 = 全部允许。 */
export function isToolAllowed(enabledTools: string[], tool: string): boolean {
  return enabledTools.length === 0 || enabledTools.includes(tool)
}

/** 停用某工具，返回新的 enabledTools。 */
export function disableTool(allTools: string[], enabledTools: string[], tool: string): string[] {
  // 当前为空 = 全部允许：先展开成全部工具名，再移除目标。
  const base = enabledTools.length === 0 ? [...allTools] : [...enabledTools]
  return base.filter((name) => name !== tool)
}

/** 允许某工具，返回新的 enabledTools。 */
export function allowTool(allTools: string[], enabledTools: string[], tool: string): string[] {
  // 当前为空 = 已是全部允许：无需变更。
  if (enabledTools.length === 0) return enabledTools
  const next = enabledTools.includes(tool) ? [...enabledTools] : [...enabledTools, tool]
  // 加完恰好覆盖全部工具 ⇒ 回到规范的「全部允许」空数组态。
  const allSet = new Set(allTools)
  if (next.length >= allTools.length && allTools.every((name) => next.includes(name)) && next.every((name) => allSet.has(name))) {
    return []
  }
  return next
}

/** 切换某工具的允许/停用态，返回新的 enabledTools。 */
export function toggleTool(
  allTools: string[],
  enabledTools: string[],
  tool: string,
  allow: boolean,
): string[] {
  return allow
    ? allowTool(allTools, enabledTools, tool)
    : disableTool(allTools, enabledTools, tool)
}
