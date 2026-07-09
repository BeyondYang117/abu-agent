# PRD：离线 OCR 高精度档（PP-OCRv6 medium）

## 背景

当前离线 OCR（RapidOCR）= `oar-ocr 0.6.3` + ONNX Runtime 动态加载，跑 PP-OCRv5 mobile 中英模型（~20MB，单张 200-500ms）。用户希望增加一个高精度离线 OCR 选项。

调研结论（2026-07-08，来源：GreatV/oar-ocr releases/PRs、paddleocr.ai v6 文档）：
- PaddleOCR 官方无 Rust 方案，官方权重非 ONNX；现有 `oar-ocr` 管线即 PaddleOCR 模型的 Rust 部署，无需新引擎。
- 高精度档选 **PP-OCRv6 medium**（det 59.2MB + rec 73MB + 字典 `ppocrv6_dict.txt` 73KB ≈ 132MiB）：精度超 PP-OCRv5 server（检测 +4.6pp / 识别 +5.1pp），纯 CPU ONNX Runtime 快近 2 倍（3.31s vs 6.36s / 张），50 语言统一（中英日 + 46 拉丁语系）。
- v6 需要 `oar-ocr >= 0.7.1`（0.7.1 才完整支持 v6）；升级到 0.8.0 对现用 API（`OAROCRBuilder::new(det, rec, keys)`、`OAROCRResult.text_regions`）无破坏；`ort =2.0.0-rc.12` 不变；MSRV 1.95（需核实本地工具链）。
- v6 必须显式设置检测阈值 `TextDetectionConfig { score_threshold: 0.2, box_threshold: 0.45, unclip_ratio: 1.4 }`（builder 默认 0.3/0.6/1.5 只适配 v3/v4/v5）。
- v6 模型下载源：ModelScope，URL 模式 `https://www.modelscope.cn/api/v1/models/greatv/oar-ocr/repo?Revision=master&FilePath=<file>`，文件 `pp-ocrv6_medium_det.onnx` / `pp-ocrv6_medium_rec.onnx` / `ppocrv6_dict.txt`。
- oar-ocr 0.7.0 的 PR #131 会轻微改变 v5 检测框数值（非 API 破坏），升级后需对现有 v5 mobile 路径做一次回归目检。

## 需求（方案 C：场景级规格选择）

1. RapidOCR 内部支持两个模型档位并存：
   - `standard` — PP-OCRv5 mobile（现状，默认，不变）
   - `high` — PP-OCRv6 medium（新增，按需单独下载）
2. **按调用场景各自选择档位**（不做全局一刀切）：
   - 截图翻译设置：RapidOCR 引擎下新增「标准 / 高精度」选择，默认标准（保速度）。
   - 知识库文档处理设置：`rapid_ocr` 引擎下新增同样选择，默认标准。
   - 替换翻译（`ocr_image_lines`）跟随截图翻译的档位设置。
3. 每个档位独立的下载/安装状态：status 按档位报告就绪情况；install 按档位下载；未就绪时沿用 `rapidocr_models_missing` 错误（前端据此显示下载入口）。
4. ONNX Runtime dylib 在两档间共享（只下载一次）。
5. 设置向后兼容：旧配置无档位字段 ⇒ 默认 standard，行为与升级前完全一致。

## 非目标

- 不新增独立"PaddleOCR"引擎枚举项（不改 `OcrMode` / `ocr_engine` 的取值集合）。
- 不做下载进度条/断点续传/SHA 校验（维持现有安装器的刻意简单）。
- 不接入 textline 方向分类、tiny/small 档、OpenVINO EP。
- 不动 System OCR / CloudVision 路径。

## 验收标准

- [ ] `cargo test`（经 `scripts/win-cargo-test.ps1`）、`npm run lint`、`npm run typecheck` 通过（对照已知基线失败）。
- [ ] 设置页两处（截图翻译、文档处理）在选 RapidOCR/rapid_ocr 时出现档位选择；高精度未安装时给下载入口，安装后状态正确。
- [ ] 截图翻译选高精度 → OCR 走 v6 medium 管线；知识库图片入库选高精度 → 同理；两场景档位互不影响。
- [ ] 旧 settings.json（无新字段）加载后默认 standard，现有 v5 mobile 用户零感知。
- [ ] 高精度模型缺失时调用返回 `rapidocr_models_missing`（含档位信息的等价形式亦可），前端提示正确。
- [ ] v5 mobile 现有路径升级 oar-ocr 后回归正常（截图翻译 + 替换翻译目检）。
