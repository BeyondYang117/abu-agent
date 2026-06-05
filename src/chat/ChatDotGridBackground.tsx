import { useEffect, useRef } from 'react'

const GRID_SPACING = 20
const DOT_RADIUS = 1
const PATTERN_MIN_SEC = 7
const PATTERN_MAX_SEC = 12

type Dot = {
  x: number
  y: number
  base: number
  phase: number
  speed: number
}

type PatternId =
  | 'band-lr'
  | 'band-rl'
  | 'band-tb'
  | 'band-bt'
  | 'band-diag'
  | 'band-diag-rev'
  | 'ring-out'
  | 'ring-in'
  | 'wave-h'
  | 'wave-v'

const PATTERN_IDS: PatternId[] = [
  'band-lr',
  'band-rl',
  'band-tb',
  'band-bt',
  'band-diag',
  'band-diag-rev',
  'ring-out',
  'ring-in',
  'wave-h',
  'wave-v',
]

type ActivePattern = {
  id: PatternId
  startedAtSec: number
  durationSec: number
}

function seededUnit(seed: number): number {
  return ((seed >>> 0) % 1000) / 1000
}

function randomBetween(min: number, max: number): number {
  return min + Math.random() * (max - min)
}

function pickNextPattern(previous?: PatternId): PatternId {
  const pool = previous ? PATTERN_IDS.filter((id) => id !== previous) : PATTERN_IDS
  return pool[Math.floor(Math.random() * pool.length)]
}

function buildDots(width: number, height: number): Dot[] {
  const dots: Dot[] = []
  for (let y = GRID_SPACING / 2; y < height; y += GRID_SPACING) {
    for (let x = GRID_SPACING / 2; x < width; x += GRID_SPACING) {
      const gx = Math.floor(x / GRID_SPACING)
      const gy = Math.floor(y / GRID_SPACING)
      const seed = (gx * 73856093) ^ (gy * 19349663)
      const depth = seededUnit(seed)
      const rhythm = seededUnit(seed * 48271)
      dots.push({
        x,
        y,
        base: 0.08 + depth * 0.14,
        phase: rhythm * Math.PI * 2,
        speed: 0.3 + depth * 0.5,
      })
    }
  }
  return dots
}

function centerFade(x: number, y: number, width: number, height: number): number {
  const nx = (x - width * 0.5) / (width * 0.425)
  const ny = (y - height * 0.42) / (height * 0.375)
  const distance = nx * nx + ny * ny
  return Math.max(0, Math.min(1, 1 - distance * 0.95))
}

function gaussianBand(value: number, center: number, sigma: number): number {
  const delta = value - center
  return Math.exp(-(delta * delta) / (2 * sigma * sigma))
}

function contentFocus(x: number, y: number, width: number, height: number): number {
  const yNorm = y / height
  const xNorm = x / width
  const vertical = 0.55 + 0.45 * Math.exp(-Math.pow((yNorm - 0.4) / 0.34, 2))
  const horizontal = 0.6 + 0.4 * Math.exp(-Math.pow((xNorm - 0.5) / 0.42, 2))
  return Math.max(vertical, horizontal * 0.85)
}

function patternProgress(localSec: number, durationSec: number): number {
  const travel = 1.32
  const start = -0.16
  return start + (localSec / durationSec) * travel
}

function linearBand(
  axisValue: number,
  axisMax: number,
  localSec: number,
  durationSec: number,
  direction: 1 | -1,
  trailOffset: number,
): number {
  const base = patternProgress(localSec, durationSec)
  const center = direction === 1 ? base : 1.16 - base
  const norm = axisValue / axisMax
  const main = gaussianBand(norm, center, 0.085)
  const trail = gaussianBand(norm, center + trailOffset * direction, 0.1) * 0.4
  return Math.min(1, main + trail)
}

function computePatternGlow(
  id: PatternId,
  x: number,
  y: number,
  width: number,
  height: number,
  localSec: number,
  durationSec: number,
): number {
  const focus = contentFocus(x, y, width, height)

  switch (id) {
    case 'band-lr':
      return linearBand(x, width, localSec, durationSec, 1, -0.11) * focus
    case 'band-rl':
      return linearBand(x, width, localSec, durationSec, -1, 0.11) * focus
    case 'band-tb':
      return linearBand(y, height, localSec, durationSec, 1, -0.11) * focus
    case 'band-bt':
      return linearBand(y, height, localSec, durationSec, -1, 0.11) * focus
    case 'band-diag': {
      const diag = (x / width + y / height) * 0.5
      const center = patternProgress(localSec, durationSec)
      const main = gaussianBand(diag, center, 0.07)
      const trail = gaussianBand(diag, center - 0.09, 0.085) * 0.38
      return Math.min(1, (main + trail) * focus)
    }
    case 'band-diag-rev': {
      const diag = (x / width + (height - y) / height) * 0.5
      const center = patternProgress(localSec, durationSec)
      const main = gaussianBand(diag, center, 0.07)
      const trail = gaussianBand(diag, center - 0.09, 0.085) * 0.38
      return Math.min(1, (main + trail) * focus)
    }
    case 'ring-out': {
      const cx = width * 0.5
      const cy = height * 0.42
      const maxR = Math.hypot(width, height) * 0.55
      const dist = Math.hypot(x - cx, y - cy) / maxR
      const center = patternProgress(localSec, durationSec)
      const main = gaussianBand(dist, center, 0.09)
      const trail = gaussianBand(dist, center - 0.08, 0.1) * 0.35
      return Math.min(1, (main + trail) * focus)
    }
    case 'ring-in': {
      const cx = width * 0.5
      const cy = height * 0.42
      const maxR = Math.hypot(width, height) * 0.55
      const dist = Math.hypot(x - cx, y - cy) / maxR
      const center = 1.16 - patternProgress(localSec, durationSec)
      const main = gaussianBand(dist, center, 0.09)
      const trail = gaussianBand(dist, center + 0.08, 0.1) * 0.35
      return Math.min(1, (main + trail) * focus)
    }
    case 'wave-h': {
      const phase = (x / width) * Math.PI * 5 - localSec * 2.4
      const wave = Math.pow(Math.max(0, Math.sin(phase)), 2.2)
      const drift = gaussianBand(x / width, patternProgress(localSec, durationSec), 0.22) * 0.35
      return Math.min(1, (wave * 0.65 + drift) * focus)
    }
    case 'wave-v': {
      const phase = (y / height) * Math.PI * 5 - localSec * 2.4
      const wave = Math.pow(Math.max(0, Math.sin(phase)), 2.2)
      const drift = gaussianBand(y / height, patternProgress(localSec, durationSec), 0.22) * 0.35
      return Math.min(1, (wave * 0.65 + drift) * focus)
    }
    default:
      return 0
  }
}

function resolvePattern(nowSec: number, active: ActivePattern): { pattern: ActivePattern; localSec: number } {
  const elapsed = nowSec - active.startedAtSec
  if (elapsed < active.durationSec) {
    return { pattern: active, localSec: elapsed }
  }

  const next: ActivePattern = {
    id: pickNextPattern(active.id),
    startedAtSec: nowSec,
    durationSec: randomBetween(PATTERN_MIN_SEC, PATTERN_MAX_SEC),
  }
  return { pattern: next, localSec: 0 }
}

function readDarkMode(): boolean {
  return document.documentElement.classList.contains('dark')
}

function prefersReducedMotion(): boolean {
  return window.matchMedia('(prefers-reduced-motion: reduce)').matches
}

export function ChatDotGridBackground() {
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const frameRef = useRef<number>()
  const dotsRef = useRef<Dot[]>([])
  const patternRef = useRef<ActivePattern | null>(null)
  const darkRef = useRef(readDarkMode())
  const reducedMotionRef = useRef(prefersReducedMotion())

  useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas) return

    const ctx = canvas.getContext('2d')
    if (!ctx) return

    const resize = () => {
      const parent = canvas.parentElement
      if (!parent) return
      const width = Math.max(1, Math.floor(parent.clientWidth))
      const height = Math.max(1, Math.floor(parent.clientHeight))
      const dpr = Math.min(window.devicePixelRatio || 1, 2)
      canvas.width = Math.floor(width * dpr)
      canvas.height = Math.floor(height * dpr)
      canvas.style.width = `${width}px`
      canvas.style.height = `${height}px`
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
      dotsRef.current = buildDots(width, height)
    }

    const draw = (time: number) => {
      const width = canvas.clientWidth
      const height = canvas.clientHeight
      if (width <= 0 || height <= 0) return

      ctx.clearRect(0, 0, width, height)
      const dark = darkRef.current
      const reducedMotion = reducedMotionRef.current
      const nowSec = time * 0.001

      if (!patternRef.current) {
        patternRef.current = {
          id: pickNextPattern(),
          startedAtSec: nowSec,
          durationSec: randomBetween(PATTERN_MIN_SEC, PATTERN_MAX_SEC),
        }
      }

      const { pattern, localSec } = resolvePattern(nowSec, patternRef.current)
      patternRef.current = pattern

      for (const dot of dotsRef.current) {
        const band = reducedMotion
          ? 0
          : computePatternGlow(pattern.id, dot.x, dot.y, width, height, localSec, pattern.durationSec)
        const pulse = reducedMotion ? 0 : Math.sin(nowSec * dot.speed + dot.phase) * 0.012
        const alpha = (dot.base * (0.48 + band * 0.52) + band * 0.34 + pulse) * centerFade(dot.x, dot.y, width, height)
        if (alpha <= 0.01) continue

        ctx.beginPath()
        ctx.arc(dot.x, dot.y, DOT_RADIUS, 0, Math.PI * 2)
        ctx.fillStyle = dark
          ? `rgba(255, 255, 255, ${alpha})`
          : `rgba(0, 0, 0, ${alpha})`
        ctx.fill()
      }
    }

    const loop = (time: number) => {
      draw(time)
      frameRef.current = window.requestAnimationFrame(loop)
    }

    resize()
    patternRef.current = null
    frameRef.current = window.requestAnimationFrame(loop)

    const resizeObserver = new ResizeObserver(() => resize())
    resizeObserver.observe(canvas.parentElement ?? canvas)

    const themeObserver = new MutationObserver(() => {
      darkRef.current = readDarkMode()
    })
    themeObserver.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ['class'],
    })

    const motionMedia = window.matchMedia('(prefers-reduced-motion: reduce)')
    const onMotionChange = () => {
      reducedMotionRef.current = motionMedia.matches
    }
    motionMedia.addEventListener('change', onMotionChange)

    return () => {
      resizeObserver.disconnect()
      themeObserver.disconnect()
      motionMedia.removeEventListener('change', onMotionChange)
      if (frameRef.current) window.cancelAnimationFrame(frameRef.current)
      patternRef.current = null
    }
  }, [])

  return (
    <canvas
      ref={canvasRef}
      className="chat-empty-hero-dot-canvas"
      aria-hidden="true"
    />
  )
}
