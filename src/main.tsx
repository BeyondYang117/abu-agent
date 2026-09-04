import React from 'react'
import ReactDOM from 'react-dom/client'
import App from './App.tsx'
import './index.css'
import 'streamdown/styles.css'
import { abuApiAuthStore } from './api/abuApiAuth'
import { api } from './api/tauri'
import { initAbuApiClient, DEFAULT_ABU_API_BASE_URL } from './api/abuApi'
import { syncModelRoutingPolicy } from './chat/modelRoutingPolicy'

// 屏蔽 WebView 原生右键菜单（Back/Reload/Inspect）
document.addEventListener('contextmenu', (e) => e.preventDefault())

// 初始化 ABU API 认证状态
api
  .loadAbuApiConfig()
  .then((config: { session_token: string | null; device_id: string | null; base_url: string | null }) => {
    abuApiAuthStore.updateFromSettings({
      sessionToken: config.session_token,
      deviceId: config.device_id,
      baseUrl: config.base_url,
    })

    // 如果已登录，初始化 ABU API 客户端
    if (config.session_token) {
      initAbuApiClient(config.base_url || DEFAULT_ABU_API_BASE_URL, config.session_token)
      void syncModelRoutingPolicy()
    }
  })
  .catch((err: unknown) => {
    console.warn('Failed to load ABU API config:', err)
  })

// React 应用入口文件
// 使用 createRoot 将 App 组件挂载到 DOM 的 #root 元素上
ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
)
