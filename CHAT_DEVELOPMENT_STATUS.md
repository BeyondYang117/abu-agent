# Kivio AI 客户端开发进度

## 已完成的工作

### 后端架构（Rust + Tauri）

✅ **数据结构定义** (`src-tauri/src/chat/types.rs`)
- `ChatMessage` - 聊天消息
- `Attachment` - 附件
- `Conversation` - 完整对话
- `ConversationListItem` - 对话列表项
- `ConversationIndex` - 对话索引

✅ **存储层** (`src-tauri/src/chat/storage.rs`)
- 对话目录管理：`{app_data_dir}/conversations/`
- 索引文件：`index.json`
- 对话文件：`{conversation_id}.json`
- 附件目录：`{conversation_id}_attachments/`
- 分页加载支持

✅ **命令接口** (`src-tauri/src/chat/commands.rs`)
- `chat_get_conversations` - 获取对话列表（支持分页、文件夹筛选）
- `chat_get_conversation` - 获取对话详情
- `chat_create_conversation` - 创建新对话
- `chat_send_message` - 发送消息（支持流式响应）
- `chat_delete_conversation` - 删除对话
- `chat_update_conversation` - 更新对话（标题、置顶、文件夹）

✅ **集成到主应用** (`src-tauri/src/main.rs`)
- 模块导入
- 命令注册
- 编译通过

### 前端架构（React + TypeScript）

✅ **类型定义** (`src/chat/types.ts`)
- 前端数据类型映射
- 对话分组类型

✅ **API 封装** (`src/chat/api.ts`)
- 封装所有 Tauri 命令调用
- 统一错误处理

✅ **工具函数** (`src/chat/utils.ts`)
- 对话按时间分组
- 相对时间格式化
- 文本截断

✅ **UI 组件**
- `Chat.tsx` - 主容器组件
- `Sidebar.tsx` - 左侧边栏（对话列表、搜索、快捷入口）
- `ConversationList.tsx` - 对话列表（分组显示）
- `MessageList.tsx` - 消息列表（空状态、自动滚动）
- `MessageBubble.tsx` - 消息气泡（支持推理、附件、时间戳）
- `InputBar.tsx` - 输入栏（支持多行、快捷键、附件按钮）
- `ModelSelector.tsx` - 模型选择器（下拉菜单）

✅ **路由集成** (`src/App.tsx`)
- 支持 `#chat` 路由
- 支持 `#chat/{conversation_id}` 子路由
- 窗口尺寸自动调整（1280×800）
- 模式切换

### 设计对齐

✅ **基于 Jan AI 的 UI 设计**
- 270px 固定宽度侧边栏
- 顶部模型选择器
- 对话按时间分组（今天、昨天、最近7天、最近30天、更早）
- 空状态居中显示
- 现代化的圆角、阴影和过渡效果
- 深色模式支持

---

## 尚未完成的功能

### 核心功能
- [ ] 流式响应集成（监听 `chat-stream` 事件）
- [ ] 附件上传和显示
- [ ] Lens 截图集成到对话中
- [ ] 对话搜索功能
- [ ] 对话删除确认
- [ ] 对话标题自动生成优化

### 启动逻辑
- [ ] 点击应用图标打开 AI 客户端（修改 `open_settings_window_for_activation`）
- [ ] 动态 Dock 图标显示/隐藏（切换 `ActivationPolicy`）
- [ ] 托盘菜单新增"打开 AI 客户端"选项

### 高级功能
- [ ] 对话文件夹管理
- [ ] 对话置顶
- [ ] 对话导出
- [ ] 快捷键支持
- [ ] 侧边栏折叠动画
- [ ] 消息重新生成
- [ ] 消息编辑
- [ ] 消息复制

### 与现有功能融合
- [ ] Lens 截图后转到 AI 客户端
- [ ] 翻译器和 AI 客户端的切换
- [ ] 设置面板集成 AI 客户端配置

---

## 下一步计划

### Phase 1: 基础功能验证（1-2天）
1. 修改启动逻辑，点击应用图标打开 Chat 界面
2. 测试对话创建、发送消息、流式响应
3. 测试对话列表加载和切换
4. 修复发现的 bug

### Phase 2: Lens 集成（2-3天）
1. Lens 截图后增加"发送到 AI 客户端"按钮
2. 将截图作为附件添加到对话中
3. 对话中支持快捷截图（调用 Lens）

### Phase 3: 完善体验（1-2周）
1. 对话搜索
2. 对话管理（删除、置顶、文件夹）
3. 附件上传和预览
4. 消息操作（重新生成、编辑、复制）
5. 侧边栏折叠/展开

### Phase 4: 高级功能（长期）
1. 多模型对比
2. Agent 模式
3. 知识库/RAG
4. Prompt 模板库

---

## 技术架构总结

**前端技术栈**：
- React 18
- TypeScript
- TailwindCSS v4
- Lucide Icons
- Vite

**后端技术栈**：
- Rust
- Tauri v2
- Serde (JSON 序列化)
- Tokio (异步运行时)
- UUID (ID 生成)

**数据流**：
```
用户操作 → React 组件 → Tauri API → Rust 命令 → 文件系统/API 调用
                                ↓
                          流式事件返回
                                ↓
                          React 监听器 → UI 更新
```

**文件结构**：
```
{app_data_dir}/conversations/
  ├── index.json                         # 对话索引
  ├── conv_xxx.json                      # 对话详情
  ├── conv_xxx_attachments/              # 对话附件
  │   ├── att_001.png
  │   └── att_002.pdf
  └── conv_yyy.json
```

---

## 如何测试

### 启动开发服务器
```bash
cd /Users/zmair/ZM\ database/keylingo/keylingo
npm run dev
```

### 访问 AI 客户端
在应用中访问 URL：`http://localhost:5713/#chat`

### 测试功能
1. ✅ 点击"新建聊天"创建对话
2. ✅ 输入消息并发送
3. ✅ 查看流式响应
4. ✅ 切换模型
5. ✅ 查看对话历史
6. ✅ 搜索对话
7. ✅ 删除对话

---

**当前状态**: ✅ 基础架构完成，前后端编译通过，等待集成测试
