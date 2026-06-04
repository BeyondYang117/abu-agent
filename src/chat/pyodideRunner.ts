import type { PyodideInterface } from 'pyodide'
import type { ChatToolArtifact } from './types'

const PYODIDE_VERSION = '0.26.4'
const PYODIDE_PRIMARY_CDN_INDEX_URL = `https://cdn.jsdelivr.net/pyodide/v${PYODIDE_VERSION}/full/`
const PYODIDE_CDN_INDEX_URLS = [
  PYODIDE_PRIMARY_CDN_INDEX_URL,
  `https://cdn.jsdelivr.net/npm/pyodide@${PYODIDE_VERSION}/`,
  `https://unpkg.com/pyodide@${PYODIDE_VERSION}/`,
]
const PYODIDE_PACKAGE_IMPORTS: Array<[RegExp, string]> = [
  [/(^|\n)\s*(import|from)\s+numpy\b/, 'numpy'],
  [/(^|\n)\s*(import|from)\s+matplotlib\b/, 'matplotlib'],
  [/(^|\n)\s*(import|from)\s+pandas\b/, 'pandas'],
  [/(^|\n)\s*(import|from)\s+scipy\b/, 'scipy'],
  [/(^|\n)\s*(import|from)\s+sympy\b/, 'sympy'],
  [/(^|\n)\s*(import|from)\s+sklearn\b/, 'scikit-learn'],
  [/(^|\n)\s*(import|from)\s+statsmodels\b/, 'statsmodels'],
  [/(^|\n)\s*(import|from)\s+(PIL|pillow)\b/, 'pillow'],
  [/(^|\n)\s*(import|from)\s+seaborn\b/, 'seaborn'],
  [/(^|\n)\s*(import|from)\s+micropip\b/, 'micropip'],
]
const PYTHON_NETWORK_CLIENT_IMPORTS = /(^|\n)\s*(import|from)\s+(tavily|requests|httpx|urllib3|aiohttp)\b/
const PYTHON_SANDBOX_FAILURE_GUIDANCE =
  '不要重试 run_python，也不要使用 run_command/pip 安装或修改本机 Python 环境来绕过沙盒；请直接基于已有数据用文本/表格回答，除非用户明确要求修改本机环境。'
const PYTHON_IMAGE_ARTIFACT_EXTENSIONS = new Set(['.png', '.jpg', '.jpeg', '.gif', '.webp', '.svg'])
const PYTHON_ARTIFACT_SCAN_ROOTS = ['/home/pyodide', '/tmp']
const MAX_PYTHON_IMAGE_ARTIFACT_BYTES = 12 * 1024 * 1024
const MAX_PYTHON_IMAGE_ARTIFACTS = 12

type PyodideRuntimeMode = 'local' | 'packages'
type PyodideFsStat = {
  mode: number
  size?: number
  mtime?: Date | number
}
type PyodideFs = {
  readdir(path: string): string[]
  stat(path: string): PyodideFsStat
  isDir(mode: number): boolean
  isFile(mode: number): boolean
  readFile(path: string, options?: { encoding?: 'binary' }): Uint8Array | ArrayBuffer | number[] | string
  analyzePath?: (path: string) => { exists: boolean }
}
type PythonArtifactSnapshotEntry = {
  size: number
  mtimeMs: number
}
type PythonOutputResult = {
  content: string
  artifacts: ChatToolArtifact[]
}

let localPyodidePromise: Promise<PyodideInterface> | null = null
let packagePyodidePromise: Promise<PyodideInterface> | null = null
let packagePyodideSourceLabel = ''

function localPyodideIndexUrl(): string {
  return new URL(`${import.meta.env.BASE_URL}pyodide/`, window.location.href).toString()
}

function compactErrorMessage(message: string): string {
  const normalized = message
    .replace(/\s+/g, ' ')
    .replace(/https?:\/\/\S+/g, (url) => {
      try {
        const parsed = new URL(url)
        return `${parsed.origin}${parsed.pathname}`
      } catch {
        return url
      }
    })
    .trim()
  if (normalized.length <= 700) return normalized
  return `${normalized.slice(0, 700)}...`
}

async function loadPyodideRuntime(mode: PyodideRuntimeMode): Promise<PyodideInterface> {
  const { loadPyodide } = await import('pyodide')
  const sources = mode === 'packages'
    ? PYODIDE_CDN_INDEX_URLS.map((indexURL) => ({ label: indexURL, indexURL }))
    : [
      { label: 'local app resources', indexURL: localPyodideIndexUrl() },
      ...PYODIDE_CDN_INDEX_URLS.map((indexURL) => ({ label: indexURL, indexURL })),
    ]
  const errors: string[] = []
  for (const source of sources) {
    try {
      const pyodide = await loadPyodide({
        indexURL: source.indexURL,
        lockFileURL: `${source.indexURL}pyodide-lock.json`,
        stdLibURL: `${source.indexURL}python_stdlib.zip`,
      })
      if (mode === 'packages') {
        packagePyodideSourceLabel = source.label
      }
      return pyodide
    } catch (err) {
      errors.push(`${source.label}: ${compactErrorMessage(describePythonError(err))}`)
    }
  }
  throw new Error(`所有 Python 运行时来源加载失败。${errors.join('；')}`)
}

function getPyodide(mode: PyodideRuntimeMode): Promise<PyodideInterface> {
  if (mode === 'packages') {
    if (!packagePyodidePromise) {
      packagePyodidePromise = loadPyodideRuntime(mode).catch((err) => {
        packagePyodidePromise = null
        throw err
      })
    }
    return packagePyodidePromise
  }
  if (!localPyodidePromise) {
    localPyodidePromise = loadPyodideRuntime(mode).catch((err) => {
      localPyodidePromise = null
      throw err
    })
  }
  return localPyodidePromise
}

export type PythonRunOutcome = {
  content: string
  isError: boolean
  artifacts: ChatToolArtifact[]
}

type PythonExecutionSuccess = {
  content: string
  artifacts: ChatToolArtifact[]
}

function describePythonError(err: unknown): string {
  if (err instanceof Error) {
    return compactErrorMessage(err.message || err.stack || err.name || String(err))
  }
  if (typeof err === 'string') return err
  try {
    return compactErrorMessage(JSON.stringify(err))
  } catch {
    return compactErrorMessage(String(err))
  }
}

function pythonStringLiteral(value: string): string {
  return JSON.stringify(value)
}

function wrapPythonUserCode(code: string): string {
  return `
import traceback as _kivio_traceback

try:
    exec(${pythonStringLiteral(code)}, globals(), globals())
except BaseException:
    _kivio_traceback.print_exc()
    raise
`.trim()
}

function stripPythonOutputLabel(message: string): string {
  return message
    .replace(/\bstderr:\s*/i, '')
    .replace(/\bstdout:\s*/i, '')
    .trim()
}

function cleanPythonExceptionSnippet(message: string): string {
  const normalized = message.replace(/\s+/g, ' ').trim()
  const stackBoundary = normalized.search(
    /\s+(?=Traceback \(most recent call last\):|File\s+"|File\s+'|await CodeRunner\(|coroutine =|new_error@|[0-9]+@wasm-function|\^+)/,
  )
  const clipped = stackBoundary >= 0 ? normalized.slice(0, stackBoundary) : normalized
  return compactErrorMessage(clipped)
}

function summarizePythonTraceback(message: string): string {
  const cleaned = stripPythonOutputLabel(message)
  const stackNoise = /(pyodide\.asm\.js|wasm-function|new_error@|_pyodide)/i
  const exceptionName = /^[A-Za-z_][\w.]*(Error|Exception|Warning|Interrupt|Exit|Fault|Found|Denied|Timeout)\b/
  const lines = cleaned
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean)
  const exceptionLine = [...lines]
    .reverse()
    .find((line) => exceptionName.test(line) && !stackNoise.test(line) && !line.startsWith('PythonError: Traceback'))
  if (exceptionLine) return cleanPythonExceptionSnippet(exceptionLine)

  const inlineExceptions = [
    ...cleaned.matchAll(
      /\b([A-Za-z_][\w.]*(?:Error|Exception|Warning|Interrupt|Exit|Fault|Found|Denied|Timeout)\b(?::\s*[^。\r\n]+)?)/g,
    ),
  ]
    .map((match) => cleanPythonExceptionSnippet(match[1] || ''))
    .filter((value) => value && !stackNoise.test(value) && !value.startsWith('PythonError: Traceback'))
  const inlineException = inlineExceptions.reverse()[0]
  return compactErrorMessage(inlineException || cleaned)
}

function detectPyodidePackages(code: string): string[] {
  const packages = PYODIDE_PACKAGE_IMPORTS
    .filter(([pattern]) => pattern.test(code))
    .map(([, packageName]) => packageName)
  return [...new Set(packages)]
}

function codeUsesMatplotlib(code: string): boolean {
  return /(^|\n)\s*(import|from)\s+matplotlib\b/.test(code)
}

function preparePythonCode(code: string): string {
  if (!codeUsesMatplotlib(code)) return code
  return `
import os
os.environ["MPLBACKEND"] = "Agg"
import matplotlib
matplotlib.use("Agg", force=True)
import matplotlib.pyplot as _kivio_matplotlib_pyplot
_kivio_matplotlib_pyplot.ioff()

${code}
`.trim()
}

async function formatPythonOutput(pyodide: PyodideInterface): Promise<PythonOutputResult> {
  const stdout = String(await pyodide.runPythonAsync('_stdout.getvalue()'))
  const stderr = String(await pyodide.runPythonAsync('_stderr.getvalue()'))
  const stdoutResult = captureInlineImageArtifacts(stdout, 'python-output')
  const stderrResult = captureInlineImageArtifacts(stderr, 'python-error-output')
  let content = ''
  if (stdoutResult.text.trim()) {
    content += `stdout:\n${stdoutResult.text}`
    if (!stdoutResult.text.endsWith('\n')) content += '\n'
  }
  if (stderrResult.text.trim()) {
    content += `stderr:\n${stderrResult.text}`
    if (!stderrResult.text.endsWith('\n')) content += '\n'
  }
  if (!content.trim()) {
    content = '(no output)\n'
  }
  return {
    content,
    artifacts: [...stdoutResult.artifacts, ...stderrResult.artifacts],
  }
}

function shouldRetryMatplotlibExecution(code: string, message: string): boolean {
  if (!codeUsesMatplotlib(code)) return false
  const lower = message.toLowerCase()
  return (
    lower.includes('pyodide.asm.js') ||
    lower.includes('wasm-function') ||
    lower.includes('matplotlib') ||
    lower.includes('backend')
  )
}

async function warmMatplotlib(pyodide: PyodideInterface): Promise<void> {
  await pyodide.runPythonAsync(`
import os
os.environ["MPLBACKEND"] = "Agg"
import matplotlib
matplotlib.use("Agg", force=True)
import matplotlib.pyplot as _kivio_matplotlib_warmup_pyplot
_kivio_matplotlib_warmup_pyplot.figure()
_kivio_matplotlib_warmup_pyplot.close("all")
`.trim())
}

async function executePython(
  pyodide: PyodideInterface,
  code: string,
  timeoutMs: number,
): Promise<PythonExecutionSuccess> {
  const fs = pyodide.FS as PyodideFs
  const initialCwd = await getPythonCwd(pyodide)
  const initialRoots = collectScanRoots(fs, initialCwd)
  const beforeArtifacts = scanImageFiles(fs, initialRoots)
  await pyodide.runPythonAsync(`
import sys
from io import StringIO
_stdout = StringIO()
_stderr = StringIO()
sys.stdout = _stdout
sys.stderr = _stderr
`)

  try {
    await Promise.race([
      pyodide.runPythonAsync(wrapPythonUserCode(code)),
      new Promise<never>((_, reject) => {
        window.setTimeout(
          () => reject(new Error(`Python execution timed out after ${timeoutMs}ms`)),
          timeoutMs,
        )
      }),
    ])
  } catch (err) {
    const output = await formatPythonOutput(pyodide)
    const message = describePythonError(err)
    const content = output.content.trim() && output.content.trim() !== '(no output)'
      ? output.content
      : message
    throw Object.assign(new Error(content), { artifacts: output.artifacts })
  }
  const output = await formatPythonOutput(pyodide)
  const finalCwd = await getPythonCwd(pyodide)
  const scanRoots = collectScanRoots(fs, finalCwd)
  const artifacts = appendUniqueArtifacts(
    output.artifacts,
    collectNewImageArtifacts(
      pyodide,
      beforeArtifacts,
      [...new Set([...initialRoots, ...scanRoots])],
      finalCwd ?? initialCwd,
    ),
  )
  return { content: output.content, artifacts }
}

function pathBasename(path: string): string {
  return path.split('/').filter(Boolean).pop() || path
}

function normalizeVirtualPath(path: string): string {
  const normalized = path.replace(/\/+/g, '/')
  if (normalized === '/') return normalized
  return normalized.replace(/\/$/, '')
}

function joinVirtualPath(parent: string, child: string): string {
  return normalizeVirtualPath(`${parent.replace(/\/$/, '')}/${child}`)
}

function imageMimeType(path: string): string {
  const lower = path.toLowerCase()
  if (lower.endsWith('.png')) return 'image/png'
  if (lower.endsWith('.jpg') || lower.endsWith('.jpeg')) return 'image/jpeg'
  if (lower.endsWith('.gif')) return 'image/gif'
  if (lower.endsWith('.webp')) return 'image/webp'
  if (lower.endsWith('.svg')) return 'image/svg+xml'
  return 'application/octet-stream'
}

function imageExtensionForMimeType(mimeType: string): string {
  if (mimeType === 'image/png') return 'png'
  if (mimeType === 'image/jpeg') return 'jpg'
  if (mimeType === 'image/gif') return 'gif'
  if (mimeType === 'image/webp') return 'webp'
  if (mimeType === 'image/svg+xml') return 'svg'
  return 'img'
}

function inferImageMimeType(bytes: Uint8Array): string | null {
  if (
    bytes.length >= 8 &&
    bytes[0] === 0x89 &&
    bytes[1] === 0x50 &&
    bytes[2] === 0x4e &&
    bytes[3] === 0x47 &&
    bytes[4] === 0x0d &&
    bytes[5] === 0x0a &&
    bytes[6] === 0x1a &&
    bytes[7] === 0x0a
  ) {
    return 'image/png'
  }
  if (bytes.length >= 3 && bytes[0] === 0xff && bytes[1] === 0xd8 && bytes[2] === 0xff) {
    return 'image/jpeg'
  }
  if (
    bytes.length >= 6 &&
    bytes[0] === 0x47 &&
    bytes[1] === 0x49 &&
    bytes[2] === 0x46 &&
    bytes[3] === 0x38
  ) {
    return 'image/gif'
  }
  if (
    bytes.length >= 12 &&
    bytes[0] === 0x52 &&
    bytes[1] === 0x49 &&
    bytes[2] === 0x46 &&
    bytes[3] === 0x46 &&
    bytes[8] === 0x57 &&
    bytes[9] === 0x45 &&
    bytes[10] === 0x42 &&
    bytes[11] === 0x50
  ) {
    return 'image/webp'
  }
  const start = new TextDecoder('utf-8', { fatal: false }).decode(bytes.slice(0, 256)).trimStart()
  if (start.startsWith('<svg') || start.startsWith('<?xml')) {
    return 'image/svg+xml'
  }
  return null
}

function fileExtension(path: string): string {
  const name = pathBasename(path)
  const index = name.lastIndexOf('.')
  return index >= 0 ? name.slice(index).toLowerCase() : ''
}

function statMtimeMs(stat: PyodideFsStat): number {
  if (stat.mtime instanceof Date) return stat.mtime.getTime()
  if (typeof stat.mtime === 'number') return stat.mtime
  return 0
}

function statSize(stat: PyodideFsStat): number {
  return typeof stat.size === 'number' && Number.isFinite(stat.size) ? stat.size : 0
}

function pathExists(fs: PyodideFs, path: string): boolean {
  try {
    if (fs.analyzePath) return fs.analyzePath(path).exists
    fs.stat(path)
    return true
  } catch {
    return false
  }
}

async function getPythonCwd(pyodide: PyodideInterface): Promise<string | null> {
  try {
    return normalizeVirtualPath(String(await pyodide.runPythonAsync('import os\nos.getcwd()')))
  } catch {
    return null
  }
}

function collectScanRoots(fs: PyodideFs, cwd: string | null): string[] {
  const roots = [cwd, ...PYTHON_ARTIFACT_SCAN_ROOTS]
    .filter((path): path is string => Boolean(path))
    .map(normalizeVirtualPath)
  return [...new Set(roots)].filter((path) => pathExists(fs, path))
}

function scanImageFiles(
  fs: PyodideFs,
  roots: string[],
): Map<string, PythonArtifactSnapshotEntry> {
  const files = new Map<string, PythonArtifactSnapshotEntry>()
  const visited = new Set<string>()
  const walk = (dir: string, depth: number) => {
    if (depth > 8 || visited.has(dir)) return
    visited.add(dir)
    let names: string[]
    try {
      names = fs.readdir(dir)
    } catch {
      return
    }
    for (const name of names) {
      if (name === '.' || name === '..') continue
      const path = joinVirtualPath(dir, name)
      let stat: PyodideFsStat
      try {
        stat = fs.stat(path)
      } catch {
        continue
      }
      if (fs.isDir(stat.mode)) {
        walk(path, depth + 1)
        continue
      }
      if (!fs.isFile(stat.mode) || !PYTHON_IMAGE_ARTIFACT_EXTENSIONS.has(fileExtension(path))) {
        continue
      }
      files.set(path, {
        size: statSize(stat),
        mtimeMs: statMtimeMs(stat),
      })
    }
  }

  for (const root of roots) {
    walk(root, 0)
  }
  return files
}

function bytesToBase64(bytes: Uint8Array): string {
  let binary = ''
  const chunkSize = 0x8000
  for (let offset = 0; offset < bytes.length; offset += chunkSize) {
    const chunk = bytes.subarray(offset, offset + chunkSize)
    binary += String.fromCharCode(...chunk)
  }
  return window.btoa(binary)
}

function base64ToBytes(value: string): Uint8Array | null {
  const compact = value.replace(/\s+/g, '')
  if (!compact || compact.length > Math.ceil(MAX_PYTHON_IMAGE_ARTIFACT_BYTES * 4 / 3) + 8) {
    return null
  }
  try {
    const binary = window.atob(compact)
    const bytes = new Uint8Array(binary.length)
    for (let index = 0; index < binary.length; index += 1) {
      bytes[index] = binary.charCodeAt(index)
    }
    return bytes
  } catch {
    return null
  }
}

function toUint8Array(content: Uint8Array | ArrayBuffer | number[] | string): Uint8Array {
  if (content instanceof Uint8Array) return content
  if (content instanceof ArrayBuffer) return new Uint8Array(content)
  if (Array.isArray(content)) return Uint8Array.from(content)
  return new TextEncoder().encode(content)
}

function artifactNameForPath(path: string, cwd: string | null): string {
  const normalized = normalizeVirtualPath(path)
  const normalizedCwd = cwd ? normalizeVirtualPath(cwd) : ''
  if (normalizedCwd && normalized.startsWith(`${normalizedCwd}/`)) {
    return normalized.slice(normalizedCwd.length + 1)
  }
  return pathBasename(normalized)
}

function imageArtifactNotice(artifact: ChatToolArtifact): string {
  const size = artifact.sizeBytes ?? artifact.size_bytes
  const sizeText = typeof size === 'number' ? `, ${size} bytes` : ''
  return `[image artifact captured: ${artifact.name}${sizeText}. Do not repeat base64 or data URLs in the final answer; Kivio renders this image automatically.]`
}

function createInlineImageArtifact(
  baseName: string,
  mimeType: string,
  bytes: Uint8Array,
  index: number,
): ChatToolArtifact {
  const suffix = index > 1 ? `-${index}` : ''
  return {
    name: `${baseName}${suffix}.${imageExtensionForMimeType(mimeType)}`,
    mimeType,
    dataUrl: `data:${mimeType};base64,${bytesToBase64(bytes)}`,
    sizeBytes: bytes.byteLength,
  }
}

function captureInlineImageArtifacts(text: string, baseName: string): { text: string; artifacts: ChatToolArtifact[] } {
  const artifacts: ChatToolArtifact[] = []
  let nextIndex = 1
  let sanitized = text.replace(
    /data:(image\/(?:png|jpe?g|gif|webp|svg\+xml));base64,([a-z0-9+/=]{128,})/gi,
    (_match, rawMimeType: string, rawBase64: string) => {
      const bytes = base64ToBytes(rawBase64)
      if (!bytes) return _match
      const mimeType = rawMimeType.toLowerCase().replace('image/jpg', 'image/jpeg')
      const inferred = inferImageMimeType(bytes)
      if (inferred && inferred !== mimeType) return _match
      const artifact = createInlineImageArtifact(baseName, mimeType, bytes, nextIndex)
      nextIndex += 1
      artifacts.push(artifact)
      return imageArtifactNotice(artifact)
    },
  )

  sanitized = sanitized
    .split('\n')
    .map((line) => {
      const trimmed = line.trim()
      if (trimmed.length < 128 || !/^[a-z0-9+/=]+$/i.test(trimmed)) return line
      const bytes = base64ToBytes(trimmed)
      if (!bytes) return line
      const mimeType = inferImageMimeType(bytes)
      if (!mimeType) return line
      const artifact = createInlineImageArtifact(baseName, mimeType, bytes, nextIndex)
      nextIndex += 1
      artifacts.push(artifact)
      const prefix = line.slice(0, line.length - line.trimStart().length)
      return `${prefix}${imageArtifactNotice(artifact)}`
    })
    .join('\n')

  return { text: sanitized, artifacts }
}

function appendUniqueArtifacts(
  artifacts: ChatToolArtifact[],
  nextArtifacts: ChatToolArtifact[],
): ChatToolArtifact[] {
  const seen = new Set(
    artifacts.map((artifact) => artifact.dataUrl ?? artifact.data_url ?? artifact.name),
  )
  const combined = [...artifacts]
  for (const artifact of nextArtifacts) {
    const key = artifact.dataUrl ?? artifact.data_url ?? artifact.name
    if (!key || seen.has(key)) continue
    seen.add(key)
    combined.push(artifact)
  }
  return combined.slice(0, MAX_PYTHON_IMAGE_ARTIFACTS)
}

function collectNewImageArtifacts(
  pyodide: PyodideInterface,
  before: Map<string, PythonArtifactSnapshotEntry>,
  roots: string[],
  cwd: string | null,
): ChatToolArtifact[] {
  const fs = pyodide.FS as PyodideFs
  const after = scanImageFiles(fs, roots)
  const changedPaths = [...after.entries()]
    .filter(([path, stat]) => {
      const previous = before.get(path)
      return !previous || previous.size !== stat.size || previous.mtimeMs !== stat.mtimeMs
    })
    .sort((a, b) => b[1].mtimeMs - a[1].mtimeMs)
    .slice(0, MAX_PYTHON_IMAGE_ARTIFACTS)

  const artifacts: ChatToolArtifact[] = []
  for (const [path, stat] of changedPaths) {
    if (stat.size > MAX_PYTHON_IMAGE_ARTIFACT_BYTES) continue
    try {
      const bytes = toUint8Array(fs.readFile(path, { encoding: 'binary' }))
      artifacts.push({
        name: artifactNameForPath(path, cwd),
        mimeType: imageMimeType(path),
        dataUrl: `data:${imageMimeType(path)};base64,${bytesToBase64(bytes)}`,
        sizeBytes: bytes.byteLength,
      })
    } catch {
      // The visible Python output remains useful even if a generated artifact cannot be read.
    }
  }
  return artifacts
}

export async function runPythonInSandbox(
  code: string,
  timeoutMs: number,
): Promise<PythonRunOutcome> {
  if (PYTHON_NETWORK_CLIENT_IMPORTS.test(code)) {
    return {
      content: 'Python 沙盒不支持联网/API 客户端库。实时搜索或网页抓取请使用 web_search / web_fetch，或使用对应 Skill 提供的脚本。',
      isError: true,
      artifacts: [],
    }
  }

  const packages = detectPyodidePackages(code)
  const preparedCode = preparePythonCode(code)
  const runtimeMode: PyodideRuntimeMode = packages.length > 0 ? 'packages' : 'local'
  let pyodide: PyodideInterface
  try {
    pyodide = await getPyodide(runtimeMode)
  } catch (err) {
    return {
      content: `Python 沙盒当前不可用：${describePythonError(err)}。${PYTHON_SANDBOX_FAILURE_GUIDANCE}`,
      isError: true,
      artifacts: [],
    }
  }

  if (packages.length > 0) {
    try {
      await pyodide.loadPackage(packages)
    } catch (err) {
      return {
        content: `Python 包加载失败：${packages.join(', ')}。基础 Python 已从 ${packagePyodideSourceLabel || 'runtime'} 加载；第三方包仍需可访问的 Pyodide 包源。${describePythonError(err)}。${PYTHON_SANDBOX_FAILURE_GUIDANCE}`,
        isError: true,
        artifacts: [],
      }
    }
  }

  try {
    if (codeUsesMatplotlib(code)) {
      await warmMatplotlib(pyodide)
    }
    const output = await executePython(pyodide, preparedCode, timeoutMs)
    return { content: output.content, isError: false, artifacts: output.artifacts }
  } catch (err) {
    const message = describePythonError(err)
    if (shouldRetryMatplotlibExecution(code, message)) {
      try {
        await warmMatplotlib(pyodide)
        const retryOutput = await executePython(pyodide, preparedCode, timeoutMs)
        return {
          content: retryOutput.content,
          isError: false,
          artifacts: retryOutput.artifacts,
        }
      } catch (retryErr) {
        const retryMessage = summarizePythonTraceback(describePythonError(retryErr))
        return {
          content: `Python 执行失败（已自动重试一次 matplotlib 初始化）：${retryMessage}。建议优先使用 Pillow / numpy 直接生成图片，除非确实需要 matplotlib 图表能力。`,
          isError: true,
          artifacts: [],
        }
      }
    }
    const lower = message.toLowerCase()
    if (lower.includes('timed out')) {
      return { content: `Python 执行超时：${message}`, isError: true, artifacts: [] }
    }
    const summary = summarizePythonTraceback(message)
    if (message.includes('SyntaxError') || lower.includes('syntaxerror')) {
      return { content: `Python 语法错误：${summary}`, isError: true, artifacts: [] }
    }
    return { content: `Python 执行失败：${summary}`, isError: true, artifacts: [] }
  }
}
