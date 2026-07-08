# Implement：RapidOCR 双档位

前置（主编排已确认）：本机 rustc 1.93 < oar-ocr 0.7+ MSRV 1.95 → 第 0 步必须先 `rustup update stable`。

## 步骤

### 0. 工具链 + 依赖升级（阻塞后续）
- [ ] `rustup update stable`，确认 `rustc --version` ≥ 1.95
- [ ] `Cargo.toml`：`oar-ocr = { version = "0.8", default-features = false, features = ["simd"] }`
- [ ] `cargo check --manifest-path src-tauri/Cargo.toml` 通过；若 0.8 API 与 0.6.3 有意外出入（builder/result），以编译错误为准调整并记录

### 1. 后端 rapidocr.rs 双档位
- [ ] `ModelTier` 枚举（Standard/High）+ `from_str`（非法 → Standard）
- [ ] `download_files(tier)`：High 档 = 共享 dylib 条目 + `high/pp-ocrv6` 三文件（ModelScope URL 见 design.md）
- [ ] ort init 抽为进程级 OnceCell；`standard_pipeline`/`high_pipeline` 双 cell
- [ ] v6 pipeline 显式 `TextDetectionConfig`（0.2 / 0.45 / 1.4；以 oar-ocr 0.8 实际 API 为准）
- [ ] `ocr_image(tier)` / `ocr_image_lines(tier)` / `install(tier)` / `status()` 分档字段
- [ ] `commands.rs`：`rapidocr_install` 加 `tier` 参数；`rapidocr_status` 返回新形状

### 2. 设置 + 调用方接线
- [ ] settings.rs：两处 `rapid_ocr_tier` 字段（serde default "standard"）+ sanitize 归一
- [ ] lens_commands.rs：`run_rapidocr_ocr` + `lens_replace_translate` 读截图翻译档位
- [ ] chat/knowledge_base/process.rs：`rapid_ocr` 分支读文档处理档位
- [ ] settings.rs 已有 serde 测试模式的话，补一个旧配置默认值测试

### 3. 前端
- [ ] `src/api/tauri.ts`：`RapidOcrStatus` 新形状 + `rapidOcrInstall(tier)`
- [ ] `ScreenshotTranslationSettings.tsx`：档位 Select + 状态面板按档位
- [ ] `DocumentProcessingPanel.tsx`：同上
- [ ] `i18n.ts` 双语字符串

### 4. 验证
- [ ] `powershell -File scripts/win-cargo-test.ps1`（对照基线失败清单，见 memory）
- [ ] `npm run lint` + `npm run typecheck`
- [ ] 手动冒烟：高精度下载 → 截图翻译切高精度出结果 → 知识库图片入库高精度 → v5 标准档回归

## 回滚

单任务单提交；出问题 `git revert`。模型文件在用户数据目录，代码回滚不受影响。
