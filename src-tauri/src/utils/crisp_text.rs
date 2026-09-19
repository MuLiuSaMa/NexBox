//! 清晰文字渲染：面向分层窗口（UpdateLayeredWindow）的 GDI+ 文字封装。
//!
//! ## 为什么需要这个模块
//!
//! 分层窗口里字体发虚通常不是输出环节的问题，而是**字形光栅化**环节的问题。
//! 常见错误做法是先用 GDI `CreateFontW` 建 `HFONT`，再用 `GdipCreateFontFromDC`
//! 转成 GDI+ 字体，然后以 `TextRenderingHintAntiAliasGridFit` 绘制。这条路径有三处模糊源：
//!
//! 1. `CreateFontW` 的 `lfHeight` 是整数，`round(font_size * scale)` 会把 125% 缩放下的
//!    16.25px 量化成 16px，字号本身就不准；
//! 2. `AntiAliasGridFit` 会把字形轮廓**吸附到整数像素网格**（为 ClearType 次像素优化设计）。
//!    当字号不是整数倍缩放时，笔画被迫落在半像素上 → 边缘发灰、发虚；
//! 3. `GetDeviceCaps(dc, 88)` 取的是 DC 的 DPI，不是窗口所在显示器的 DPI，
//!    多显示器不同缩放时会取错。
//!
//! 本模块改用 GDI+ 原生字体族（`GdipCreateFontFamilyFromName` + `GdipCreateFont`），
//! 以 `UnitPixel` 传递 **f32 浮点字号**（不做 round），并用 `TextRenderingHintAntiAlias`
//! （纯灰度反锯齿，不做网格吸附）。
//!
//! 注意：不要改用 `TextRenderingHintClearTypeGridFit` —— 彩色次像素渲染在透明分层窗口上
//! 会产生明显彩边，这也是既有代码注释里已经指出的坑。

#![cfg(windows)]

use std::ptr;

use windows_sys::core::PCWSTR;
use windows_sys::Win32::Graphics::GdiPlus::*;

/// 将 UTF-8 字符串转为 NUL 结尾的 UTF-16，供 Win32 / GDI+ 宽字符 API 使用。
pub fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// 一个在分层窗口上绘制清晰文字所需的 GDI+ 字体。
///
/// 持有 `GpFontFamily` / `GpFont` 两个 GDI+ 句柄，Drop 时自动释放。
/// 字体以 `UnitPixel` 创建，字号是 f32 浮点值，**不做整数取整**，
/// 因此在 125% / 150% 等非整数倍缩放下也不会被量化。
pub struct CrispFont {
    family: *mut GpFontFamily,
    font: *mut GpFont,
}

impl CrispFont {
    /// 创建字体。`family_name` 为字体族名（如 "Microsoft YaHei UI"、"MiSans"）；
    /// `size_px` 为**已乘过 DPI 缩放**的像素字号。
    ///
    /// 字体族不存在时返回 `None`，调用方应回退到别的字体族。
    pub unsafe fn new(family_name: &str, size_px: f32) -> Option<Self> {
        let wide = to_wide(family_name);

        let mut family: *mut GpFontFamily = ptr::null_mut();
        if GdipCreateFontFamilyFromName(
            wide.as_ptr() as PCWSTR,
            ptr::null_mut(),
            &mut family,
        ) != 0
            || family.is_null()
        {
            return None;
        }

        // FontStyleRegular = 0；UnitPixel = 2（比 UnitPoint 更直观：直接给像素值）
        let mut font: *mut GpFont = ptr::null_mut();
        if GdipCreateFont(
            family,
            size_px,
            FontStyleRegular,
            UnitPixel,
            &mut font,
        ) != 0
            || font.is_null()
        {
            GdipDeleteFontFamily(family);
            return None;
        }

        Some(CrispFont { family, font })
    }

    pub fn handle(&self) -> *mut GpFont {
        self.font
    }
}

impl Drop for CrispFont {
    fn drop(&mut self) {
        unsafe {
            if !self.font.is_null() {
                GdipDeleteFont(self.font);
                self.font = ptr::null_mut();
            }
            if !self.family.is_null() {
                GdipDeleteFontFamily(self.family);
                self.family = ptr::null_mut();
            }
        }
    }
}

/// 在给定 `GpGraphics` 上把文字渲染提示设成**不做网格吸附**的纯灰度反锯齿。
///
/// 这是消除边缘发虚的关键一步：`AntiAliasGridFit` 会把字形吸附到整数像素网格，
/// 在非整数倍 DPI 缩放下导致笔画半像素化、边缘发灰。
pub unsafe fn set_crisp_text_hint(graphics: *mut GpGraphics) {
    GdipSetTextRenderingHint(graphics, TextRenderingHintAntiAlias);
}
