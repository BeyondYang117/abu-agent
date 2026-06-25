// 知识库引用 `[n]` 的 mdast 切分逻辑 + remark 插件。
// 抽成独立模块（非组件），既便于单测，也避免 ChatMarkdown.tsx 触发
// react-refresh/only-export-components。

// 极简 mdast 节点视图：只需 type/value/url/children 来切分文本节点。
export interface MdNode {
  type: string
  value?: string
  url?: string
  children?: MdNode[]
}

/** 把一个文本节点里的 `[n]`（且 n 是有效引用）切成 text / link 混排。 */
export function splitCitations(value: string, validNs: Set<number>): MdNode[] {
  const out: MdNode[] = []
  let last = 0
  const re = /\[(\d{1,3})\]/g
  let m: RegExpExecArray | null
  while ((m = re.exec(value))) {
    const n = Number(m[1])
    if (!validNs.has(n)) continue
    if (m.index > last) out.push({ type: 'text', value: value.slice(last, m.index) })
    out.push({ type: 'link', url: `#kb-cite-${n}`, children: [{ type: 'text', value: `[${n}]` }] })
    last = m.index + m[0].length
  }
  if (last < value.length) out.push({ type: 'text', value: value.slice(last) })
  return out
}

/** remark 插件：遍历树，把 text 节点里的有效 `[n]` 换成 `#kb-cite-n` 链接。
 *  跳过 link/code，避免嵌套链接或污染代码。 */
export function remarkCitations(validNs: Set<number>) {
  const walk = (node: MdNode) => {
    if (!node.children || node.type === 'link' || node.type === 'linkReference') return
    const next: MdNode[] = []
    for (const child of node.children) {
      if (child.type === 'text' && child.value && /\[\d{1,3}\]/.test(child.value)) {
        next.push(...splitCitations(child.value, validNs))
      } else {
        walk(child)
        next.push(child)
      }
    }
    node.children = next
  }
  return () => (tree: MdNode) => walk(tree)
}
