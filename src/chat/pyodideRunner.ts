import type { PyodideInterface } from 'pyodide'

const PYODIDE_VERSION = '0.26.4'
const PYODIDE_INDEX_URL = `https://cdn.jsdelivr.net/pyodide/v${PYODIDE_VERSION}/full/`
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

let pyodidePromise: Promise<PyodideInterface> | null = null

async function loadPyodideRuntime(): Promise<PyodideInterface> {
  const { loadPyodide } = await import('pyodide')
  return loadPyodide({ indexURL: PYODIDE_INDEX_URL })
}

function getPyodide(): Promise<PyodideInterface> {
  if (!pyodidePromise) {
    pyodidePromise = loadPyodideRuntime().catch((err) => {
      pyodidePromise = null
      throw err
    })
  }
  return pyodidePromise
}

export type PythonRunOutcome = {
  content: string
  isError: boolean
}

function describePythonError(err: unknown): string {
  if (err instanceof Error) {
    return err.message || err.stack || err.name || String(err)
  }
  if (typeof err === 'string') return err
  try {
    return JSON.stringify(err)
  } catch {
    return String(err)
  }
}

function detectPyodidePackages(code: string): string[] {
  const packages = PYODIDE_PACKAGE_IMPORTS
    .filter(([pattern]) => pattern.test(code))
    .map(([, packageName]) => packageName)
  return [...new Set(packages)]
}

async function formatPythonOutput(pyodide: PyodideInterface): Promise<string> {
  const stdout = String(await pyodide.runPythonAsync('_stdout.getvalue()'))
  const stderr = String(await pyodide.runPythonAsync('_stderr.getvalue()'))
  let content = ''
  if (stdout.trim()) {
    content += `stdout:\n${stdout}`
    if (!stdout.endsWith('\n')) content += '\n'
  }
  if (stderr.trim()) {
    content += `stderr:\n${stderr}`
    if (!stderr.endsWith('\n')) content += '\n'
  }
  if (!content.trim()) {
    content = '(no output)\n'
  }
  return content
}

export async function runPythonInSandbox(
  code: string,
  timeoutMs: number,
): Promise<PythonRunOutcome> {
  try {
    const pyodide = await getPyodide()
    const packages = detectPyodidePackages(code)
    if (packages.length > 0) {
      await pyodide.loadPackage(packages)
    }
    await pyodide.runPythonAsync(`
import sys
from io import StringIO
_stdout = StringIO()
_stderr = StringIO()
sys.stdout = _stdout
sys.stderr = _stderr
`)

    await Promise.race([
      pyodide.runPythonAsync(code),
      new Promise<never>((_, reject) => {
        window.setTimeout(
          () => reject(new Error(`Python execution timed out after ${timeoutMs}ms`)),
          timeoutMs,
        )
      }),
    ])
    const content = await formatPythonOutput(pyodide)
    return { content, isError: false }
  } catch (err) {
    const message = describePythonError(err)
    const lower = message.toLowerCase()
    if (lower.includes('timed out')) {
      return { content: `Python 执行超时：${message}`, isError: true }
    }
    if (message.includes('SyntaxError') || lower.includes('syntaxerror')) {
      return { content: `Python 语法错误：${message}`, isError: true }
    }
    if (
      lower.includes('pyodide') ||
      lower.includes('failed to fetch') ||
      lower.includes('network') ||
      lower.includes('loading')
    ) {
      return { content: `Python 环境加载失败：${message}`, isError: true }
    }
    return { content: `Python 执行失败：${message}`, isError: true }
  }
}
