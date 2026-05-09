import { ARROW_COLOR, ARROW_HEAD_ANGLE_DEG } from './annotation'
import type { Arrow } from './types'

export function ArrowSvg({ arrow }: { arrow: Arrow }) {
  const { x1, y1, x2, y2 } = arrow
  const dx = x2 - x1
  const dy = y2 - y1
  const len = Math.hypot(dx, dy)
  if (len < 1) return null

  // SVG 在逻辑像素坐标系下渲染 → 线宽用屏幕粗细,合成时再按 PNG 物理像素重算
  const lineWidth = 4
  const headSize = lineWidth * 4
  const angle = Math.atan2(dy, dx)
  const headAngle = (ARROW_HEAD_ANGLE_DEG * Math.PI) / 180

  const shaftEndX = x2 - Math.cos(angle) * (headSize * 0.6)
  const shaftEndY = y2 - Math.sin(angle) * (headSize * 0.6)
  const wing1X = x2 - Math.cos(angle - headAngle) * headSize
  const wing1Y = y2 - Math.sin(angle - headAngle) * headSize
  const wing2X = x2 - Math.cos(angle + headAngle) * headSize
  const wing2Y = y2 - Math.sin(angle + headAngle) * headSize

  return (
    <g>
      <line
        x1={x1}
        y1={y1}
        x2={shaftEndX}
        y2={shaftEndY}
        stroke={ARROW_COLOR}
        strokeWidth={lineWidth}
        strokeLinecap="round"
      />
      <polygon
        points={`${x2},${y2} ${wing1X},${wing1Y} ${wing2X},${wing2Y}`}
        fill={ARROW_COLOR}
      />
    </g>
  )
}
