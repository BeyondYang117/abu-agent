import { describe, it, expect } from 'vitest'
import { splitCitations, remarkCitations, type MdNode } from './citations'

describe('splitCitations', () => {
  it('splits valid [n] into link nodes, leaves text around', () => {
    const out = splitCitations('see [1] and [2] here', new Set([1, 2]))
    expect(out.map((n) => n.type)).toEqual(['text', 'link', 'text', 'link', 'text'])
    expect(out[1]).toMatchObject({ type: 'link', url: '#kb-cite-1' })
    expect(out[3]).toMatchObject({ type: 'link', url: '#kb-cite-2' })
  })

  it('leaves unknown citation numbers as plain text', () => {
    const out = splitCitations('ref [9] only', new Set([1]))
    expect(out).toEqual([{ type: 'text', value: 'ref [9] only' }])
  })

  it('handles adjacent citations with no text between', () => {
    const out = splitCitations('[1][2]', new Set([1, 2]))
    expect(out.map((n) => n.type)).toEqual(['link', 'link'])
  })
})

describe('remarkCitations', () => {
  it('rewrites text children but skips inside link/code nodes', () => {
    const tree: MdNode = {
      type: 'root',
      children: [
        { type: 'paragraph', children: [{ type: 'text', value: 'a [1] b' }] },
        { type: 'link', url: 'x', children: [{ type: 'text', value: 'keep [1]' }] },
        { type: 'inlineCode', value: 'arr[1]' },
      ],
    }
    remarkCitations(new Set([1]))()(tree)
    // paragraph: text split into [text, link, text]
    expect(tree.children![0].children!.map((n) => n.type)).toEqual(['text', 'link', 'text'])
    // link's inner text is untouched (no nested link)
    expect(tree.children![1].children).toEqual([{ type: 'text', value: 'keep [1]' }])
    // inlineCode leaf untouched
    expect(tree.children![2]).toEqual({ type: 'inlineCode', value: 'arr[1]' })
  })
})
