# Design：RapidOCR 双档位（v5 mobile + v6 medium）

## 核心决策

- 不新造引擎枚举，`ModelTier { Standard, High }` 是 RapidOCR 客户端的内部参数，由各调用场景的设置决定。
- crate 升级：`oar-ocr = 0.6.3 → 0.8.0`（v6 支持始于 0.7.1；0.8.0 含后续 fix）。`ort =2.0.0-rc.12` / feature 集不变。可额外开 `features = ["simd"]`（纯 CPU 加速，opt-in）。前置：`rustup update`（MSRV 1.95，本机 1.93）。

## 文件布局（`{app_data}/rapidocr-models/`）

```
rapidocr-models/
  onnxruntime.dll|libonnxruntime.dylib   # 共享，两档只下载一次（+ Windows providers_shared）
  det.onnx / rec.onnx / keys.txt         # standard = PP-OCRv5 mobile（现状位置不动，零迁移）
  high/
    det.onnx / rec.onnx / keys.txt       # high = PP-OCRv6 medium
```

高精度下载源（ModelScope）：
`https://www.modelscope.cn/api/v1/models/greatv/oar-ocr/repo?Revision=master&FilePath=pp-ocrv6_medium_det.onnx`（~59.2MB）、`...=pp-ocrv6_medium_rec.onnx`（~73MB）、`...=ppocrv6_dict.txt`（~73KB）。

## rapidocr.rs 结构变化

- `download_files()` → `download_files(tier)`：standard 返回现状清单；high 返回 dylib（共享条目，根目录）+ `high/` 下三个 v6 文件。install 已有的 file_is_ready 跳过逻辑天然实现"dylib 只下一次"。
- `pipeline: OnceCell<Arc<OAROCR>>` → 两个 cell：`standard_pipeline` / `high_pipeline`；**ort::init_from 抽成独立的进程级一次性 init**（自己的 OnceCell/Once），两个 pipeline 构建前都先确保它完成。
- v6 pipeline 构建必须显式设置检测阈值（builder 默认只适配 v3~v5）：`score_threshold=0.2, box_threshold=0.45, unclip_ratio=1.4`（用 oar-ocr 0.8 实际 API，实现时查 README/docs 确认方法名）。
- `ocr_image` / `ocr_image_lines` 增加 `tier` 参数。
- `status()` 返回每档就绪状态：`{ standardAvailable, highAvailable, modelDir }`（原 `modelsAvailable` 字段废弃，前端同步改）。
- `install(tier)`。install_lock 仍然全局一把（两档共用，防并发写 .tmp 即可）。
- 错误码：模型缺失统一返回 `rapidocr_models_missing`（前端已有处理）；status 的分档字段负责告诉前端缺的是哪档。

## 设置（settings.rs）

- `ScreenshotTranslationConfig` + `rapid_ocr_tier: String`（`"standard"`|`"high"`，serde default `"standard"`）。
- `DocumentProcessingConfig` + `rapid_ocr_tier: String`（同上）。
- `sanitize_settings`：非法值归一 `"standard"`。旧配置无字段 → default → 零感知。

## 调用方接线

- `lens_commands.rs`：`run_rapidocr_ocr` 与 `lens_replace_translate` 都读 `settings.screenshot_translation.rapid_ocr_tier` → 传给 `ocr_image` / `ocr_image_lines`。
- `chat/knowledge_base/process.rs`：`rapid_ocr` 分支读 `document_processing.rapid_ocr_tier`。
- `commands.rs`：`rapidocr_status`（返回新形状）、`rapidocr_install(tier: String)`。

## 前端

- `src/api/tauri.ts`：`RapidOcrStatus` 类型改为分档字段；`rapidOcrInstall(tier: 'standard' | 'high')`。
- `ScreenshotTranslationSettings.tsx`：`ocrMode === 'rapid_ocr'` 时显示档位 Select（标准（快）/ 高精度（v6，~139MB））；`RapidOcrStatusPanel` 按当前选中档位显示就绪状态/下载按钮。
- `DocumentProcessingPanel.tsx`：`ocr_engine === 'rapid_ocr'` 时同样的档位 Select + `RapidOcrWidget` 按档位显示。
- `src/settings/i18n.ts` 加双语字符串。

## 回归风险

- oar-ocr 0.7.0 PR #131 微调了 DB 检测后处理数值 → 升级后 v5 mobile 路径需目检（截图翻译 + 替换翻译）。
- `ort::init_from` 仍是全进程一次，失败重启——不变，两档共享同一次 init。
