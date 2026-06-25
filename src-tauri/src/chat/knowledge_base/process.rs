//! Document → text routing (Kivio 内置 only). Built-in `parse::parse_file`
//! handles txt/md/html/docx/xlsx/pdf-text. Image files are OCR'd via the
//! configured engine (system / RapidOCR). Third-party processors are suspended.
use std::path::Path;

use crate::settings::DocumentProcessingConfig;
use crate::state::AppState;

use super::parse::{self, ParsedDoc};

const IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "webp", "bmp", "tif", "tiff", "gif"];

fn ext_lower(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default()
}

pub fn is_image_ext(path: &Path) -> bool {
    IMAGE_EXTS.contains(&ext_lower(path).as_str())
}

pub async fn process_document(
    state: &AppState,
    cfg: &DocumentProcessingConfig,
    path: &Path,
) -> Result<ParsedDoc, String> {
    if is_image_ext(path) {
        let text = ocr_image(state, path, &cfg.ocr_engine).await?;
        if text.trim().is_empty() {
            return Err("OCR 未识别到文字".into());
        }
        return Ok(ParsedDoc {
            text,
            markdown: false,
        });
    }
    // PDF force_ocr 策略：内置无法栅格化扫描版 PDF（未引入 pdfium）。
    if ext_lower(path) == "pdf" && cfg.pdf_strategy == "force_ocr" {
        return Err("强制 OCR 重扫扫描版 PDF 暂未启用（内置仅支持 PDF 文字层）".into());
    }
    parse::parse_file(path)
}

/// OCR one image via the selected engine, mirroring lens_commands' dispatch.
async fn ocr_image(state: &AppState, path: &Path, engine: &str) -> Result<String, String> {
    match engine {
        "system" => {
            #[cfg(target_os = "macos")]
            {
                return state.macos_ocr.ocr_image(&path.to_string_lossy()).await;
            }
            #[cfg(target_os = "windows")]
            {
                return crate::windows_ocr::ocr_image(path).await;
            }
            #[cfg(not(any(target_os = "macos", target_os = "windows")))]
            {
                let _ = (state, path);
                return Err("系统 OCR 在此平台不可用".into());
            }
        }
        "rapid_ocr" => state.rapidocr.ocr_image(path).await,
        _ => Err("图片入库需先在「文档处理」选择 OCR 引擎".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;
    use crate::state::AppState;
    use std::path::PathBuf;

    #[test]
    fn image_ext_detection() {
        assert!(is_image_ext(&PathBuf::from("a.png")));
        assert!(is_image_ext(&PathBuf::from("photo.JPG")));
        assert!(is_image_ext(&PathBuf::from("scan.tiff")));
        assert!(!is_image_ext(&PathBuf::from("doc.pdf")));
        assert!(!is_image_ext(&PathBuf::from("notes.md")));
        assert!(!is_image_ext(&PathBuf::from("noext")));
    }

    // engine "off" returns before touching state/disk, so a headless state is
    // enough — no network, no OCR engine needed.
    #[tokio::test]
    async fn off_engine_routes_image_to_select_engine_error() {
        let state = AppState::new_headless(Settings::default(), std::env::temp_dir());
        let cfg = DocumentProcessingConfig {
            ocr_engine: "off".into(),
            pdf_strategy: "text".into(),
        };
        let err = match process_document(&state, &cfg, &PathBuf::from("/tmp/x.png")).await {
            Ok(_) => panic!("expected error"),
            Err(e) => e,
        };
        assert!(err.contains("选择 OCR 引擎"), "got: {err}");
    }

    #[tokio::test]
    async fn pdf_force_ocr_returns_honest_error() {
        let state = AppState::new_headless(Settings::default(), std::env::temp_dir());
        let cfg = DocumentProcessingConfig {
            ocr_engine: "off".into(),
            pdf_strategy: "force_ocr".into(),
        };
        let err = match process_document(&state, &cfg, &PathBuf::from("/tmp/scan.pdf")).await {
            Ok(_) => panic!("expected error"),
            Err(e) => e,
        };
        assert!(err.contains("强制 OCR"), "got: {err}");
    }
}
