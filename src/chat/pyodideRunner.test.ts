import { describe, expect, it } from 'vitest'
import { wrapPythonUserCode } from './pyodideRunner'

describe('wrapPythonUserCode', () => {
  it('suppresses non-fatal dependency warnings before executing user code', () => {
    const wrapped = wrapPythonUserCode('print("ok")')

    expect(wrapped).toContain('import warnings as _kivio_warnings')
    expect(wrapped).toContain('DeprecationWarning')
    expect(wrapped).toContain('PendingDeprecationWarning')
    expect(wrapped).toContain('FutureWarning')
    expect(wrapped).toContain('ResourceWarning')
    expect(wrapped).toContain('_kivio_warnings.filterwarnings("ignore"')
    expect(wrapped).toContain('exec("print')
  })
})
