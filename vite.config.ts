import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { defineConfig, type Plugin } from 'vite'
import react from '@vitejs/plugin-react'

const dirname = path.dirname(fileURLToPath(import.meta.url))
const pyodideDir = path.resolve(dirname, 'node_modules/pyodide')
const pyodideAssetFiles = [
  'pyodide.asm.js',
  'pyodide.asm.wasm',
  'pyodide-lock.json',
  'python_stdlib.zip',
]

function contentTypeForPyodideAsset(fileName: string): string {
  if (fileName.endsWith('.wasm')) return 'application/wasm'
  if (fileName.endsWith('.zip')) return 'application/zip'
  if (fileName.endsWith('.json')) return 'application/json; charset=utf-8'
  return 'text/javascript; charset=utf-8'
}

function pyodideAssetsPlugin(): Plugin {
  return {
    name: 'kivio-pyodide-assets',
    configureServer(server) {
      server.middlewares.use('/pyodide/', (req, res, next) => {
        const fileName = path.basename(decodeURIComponent((req.url ?? '').split('?')[0]))
        if (!pyodideAssetFiles.includes(fileName)) {
          res.statusCode = 404
          res.end(`Pyodide asset not found: ${fileName}`)
          return
        }
        const filePath = path.join(pyodideDir, fileName)
        res.setHeader('Content-Type', contentTypeForPyodideAsset(fileName))
        fs.createReadStream(filePath)
          .on('error', next)
          .pipe(res)
      })
    },
    generateBundle() {
      for (const fileName of pyodideAssetFiles) {
        this.emitFile({
          type: 'asset',
          fileName: `pyodide/${fileName}`,
          source: fs.readFileSync(path.join(pyodideDir, fileName)),
        })
      }
    },
  }
}

// https://vitejs.dev/config/
// Vite 构建配置：使用 @vitejs/plugin-react 插件支持 React 开发
export default defineConfig({
  base: './',
  plugins: [react(), pyodideAssetsPlugin()],
  build: {
    target: 'esnext',
  },
})
