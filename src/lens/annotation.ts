import type { Arrow } from './types'

export const ARROW_COLOR = '#ff3b30'
export const ARROW_HEAD_ANGLE_DEG = 30
export const ARROW_MIN_DRAG_PX = 8

function drawArrow(
  ctx: CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D,
  x1: number,
  y1: number,
  x2: number,
  y2: number,
  lineWidth: number,
) {
  const dx = x2 - x1
  const dy = y2 - y1
  const len = Math.hypot(dx, dy)
  if (len < 1) return

  const headSize = lineWidth * 4
  const angle = Math.atan2(dy, dx)
  const headAngle = (ARROW_HEAD_ANGLE_DEG * Math.PI) / 180

  // 箭杆终点回退一格,避免三角覆盖时尾端有缺口
  const shaftEndX = x2 - Math.cos(angle) * (headSize * 0.6)
  const shaftEndY = y2 - Math.sin(angle) * (headSize * 0.6)

  ctx.save()
  ctx.strokeStyle = ARROW_COLOR
  ctx.fillStyle = ARROW_COLOR
  ctx.lineWidth = lineWidth
  ctx.lineCap = 'round'
  ctx.lineJoin = 'round'

  ctx.beginPath()
  ctx.moveTo(x1, y1)
  ctx.lineTo(shaftEndX, shaftEndY)
  ctx.stroke()

  // 三角箭头
  const wing1X = x2 - Math.cos(angle - headAngle) * headSize
  const wing1Y = y2 - Math.sin(angle - headAngle) * headSize
  const wing2X = x2 - Math.cos(angle + headAngle) * headSize
  const wing2Y = y2 - Math.sin(angle + headAngle) * headSize
  ctx.beginPath()
  ctx.moveTo(x2, y2)
  ctx.lineTo(wing1X, wing1Y)
  ctx.lineTo(wing2X, wing2Y)
  ctx.closePath()
  ctx.fill()

  ctx.restore()
}

export async function composeAnnotatedImage(
  imageDataUrl: string,
  arrows: Arrow[],
  frameWidth: number,
  frameHeight: number,
): Promise<string> {
  const img = await new Promise<HTMLImageElement>((resolve, reject) => {
    const el = new Image()
    el.onload = () => resolve(el)
    el.onerror = () => reject(new Error('failed to load image for compose'))
    el.src = imageDataUrl
  })

  const canvas = new OffscreenCanvas(img.naturalWidth, img.naturalHeight)
  const ctx = canvas.getContext('2d')
  if (!ctx) throw new Error('OffscreenCanvas 2d context unavailable')

  ctx.drawImage(img, 0, 0)

  // 逻辑像素 → 物理像素的等比缩放
  // capturedFrame.width 是逻辑像素;PNG 是物理像素 → naturalWidth 大于等于 width
  const scaleX = frameWidth > 0 ? img.naturalWidth / frameWidth : 1
  const scaleY = frameHeight > 0 ? img.naturalHeight / frameHeight : 1
  const lineWidth = Math.max(3, img.naturalWidth / 400)

  for (const a of arrows) {
    drawArrow(
      ctx,
      a.x1 * scaleX,
      a.y1 * scaleY,
      a.x2 * scaleX,
      a.y2 * scaleY,
      lineWidth,
    )
  }

  const blob = await canvas.convertToBlob({ type: 'image/png' })
  const buf = await blob.arrayBuffer()
  let binary = ''
  const bytes = new Uint8Array(buf)
  const chunkSize = 0x8000
  for (let i = 0; i < bytes.length; i += chunkSize) {
    binary += String.fromCharCode(...bytes.subarray(i, i + chunkSize))
  }
  return btoa(binary)
}
