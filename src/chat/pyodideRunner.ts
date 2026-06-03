import type { PyodideInterface } from 'pyodide'

const PYODIDE_VERSION = '0.26.4'
const PYODIDE_INDEX_URL = `https://cdn.jsdelivr.net/pyodide/v${PYODIDE_VERSION}/full/`

let pyodidePromise: Promise<PyodideInterface> | null = null

async function loadPyodideRuntime(): Promise<PyodideInterface> {
  const { loadPyodide } = await import('pyodide')
  return loadPyodide({ indexURL: PYODIDE_INDEX_URL })
}

function getPyodide(): Promise<PyodideInterface> {
  if (!pyodidePromise) {
    pyodidePromise = loadPyodideRuntime()
  }
  return pyodidePromise
}

export type PythonRunOutcome = {
  content: string
  isError: boolean
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
  const pyodide = await getPyodide()
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
    const message = err instanceof Error ? err.message : String(err)
    return { content: message, isError: true }
  }
}
