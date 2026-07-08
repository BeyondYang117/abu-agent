<#
.SYNOPSIS
  为 RapidOCR 真实 E2E 测试生成一张白底黑字的中英混排 PNG 测试图。

.DESCRIPTION
  仅供 src-tauri/src/rapidocr.rs 里 RAPIDOCR_E2E=1 门控的真实推理测试使用。
  用 .NET System.Drawing 画两行文字（中文 + 英文数字），落盘到调用方指定路径。

.PARAMETER OutputPath
  生成的 PNG 完整路径。

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File scripts/gen-rapidocr-e2e-image.ps1 -OutputPath C:\temp\test.png
#>
param(
  [Parameter(Mandatory = $true)]
  [string]$OutputPath
)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

$bmp = [System.Drawing.Bitmap]::new(640, 180)
$g = [System.Drawing.Graphics]::FromImage($bmp)
try {
  $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
  $g.TextRenderingHint = [System.Drawing.Text.TextRenderingHint]::AntiAliasGridFit
  $g.Clear([System.Drawing.Color]::White)

  $font = [System.Drawing.Font]::new('Microsoft YaHei', 28, [System.Drawing.FontStyle]::Regular)
  $brush = [System.Drawing.Brushes]::Black

  $g.DrawString('离线识别测试 高精度模型', $font, $brush, 20, 30)
  $g.DrawString('Kivio RapidOCR Test 2026', $font, $brush, 20, 100)
  $g.Flush()

  $bmp.Save($OutputPath, [System.Drawing.Imaging.ImageFormat]::Png)
} finally {
  $g.Dispose()
  $bmp.Dispose()
}

Write-Host "[gen-rapidocr-e2e-image] wrote $OutputPath"
