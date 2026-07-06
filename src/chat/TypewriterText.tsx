import { useEffect, useState } from 'react'
import { prefersReducedMotion } from './utils'

interface TypewriterTextProps {
  text: string
  active?: boolean
}

const CHAR_DELAY_MS = 42
const START_DELAY_MS = 120

export function TypewriterText({ text, active = true }: TypewriterTextProps) {
  const [displayed, setDisplayed] = useState('')
  const [done, setDone] = useState(false)

  useEffect(() => {
    if (!active) {
      setDisplayed('')
      setDone(true)
      return
    }

    if (prefersReducedMotion() || !text) {
      setDisplayed(text)
      setDone(true)
      return
    }

    setDisplayed('')
    setDone(false)
    let index = 0
    let charTimer: ReturnType<typeof setTimeout> | undefined

    const typeNext = () => {
      index += 1
      setDisplayed(text.slice(0, index))
      if (index < text.length) {
        const ch = text[index - 1]
        const pause = ch === ' ' || ch === '—' || ch === '?' || ch === '.' ? CHAR_DELAY_MS + 80 : CHAR_DELAY_MS
        charTimer = setTimeout(typeNext, pause)
      } else {
        setDone(true)
      }
    }

    const startTimer = setTimeout(typeNext, START_DELAY_MS)

    return () => {
      clearTimeout(startTimer)
      clearTimeout(charTimer)
    }
  }, [text, active])

  return (
    <span>
      {displayed}
      {active && !done && (
        <span className="chat-typewriter-cursor" aria-hidden="true">
          |
        </span>
      )}
    </span>
  )
}
