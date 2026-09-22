//! The floating "● Listening…" pill.
//!
//! A native layered window (per-pixel alpha, antialiased rounded corners)
//! rendered with GDI on its own thread. It never takes focus, ignores the
//! mouse, and costs nothing while hidden — no WebView involved.

use std::sync::{Arc, Mutex, OnceLock};

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, CreateFontW, DeleteDC, DeleteObject, GetDC, GetMonitorInfoW,
    GetTextExtentPoint32W, MonitorFromWindow, ReleaseDC, SelectObject, SetBkMode, SetTextColor, TextOutW, AC_SRC_ALPHA,
    AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS,
    DEFAULT_CHARSET, DIB_RGB_COLORS, FW_NORMAL, FW_SEMIBOLD, HDC, HFONT, HGDIOBJ, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    OUT_DEFAULT_PRECIS, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, SetThreadDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetForegroundWindow, GetMessageW, KillTimer, PostMessageW,
    RegisterClassW, SetTimer, SetWindowPos, ShowWindow, TranslateMessage, UpdateLayeredWindow, HWND_TOPMOST, MSG,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, SW_HIDE, SW_SHOWNOACTIVATE, ULW_ALPHA, WM_APP, WM_TIMER,
    WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};

use super::wide;
use crate::IndicatorState;

const WM_UPDATE: u32 = WM_APP + 10;
const HIDE_TIMER: usize = 1;

struct Inner {
    state: IndicatorState,
    top: bool,
}

static SHARED: OnceLock<Arc<Mutex<Inner>>> = OnceLock::new();

pub struct Indicator {
    hwnd: isize,
    inner: Arc<Mutex<Inner>>,
}

impl Indicator {
    pub fn start() -> Indicator {
        let inner = SHARED.get_or_init(|| Arc::new(Mutex::new(Inner { state: IndicatorState::Hidden, top: false }))).clone();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name("indicator".into())
            .spawn(move || unsafe {
                let _ = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
                let hinst = GetModuleHandleW(None).unwrap_or_default();
                let class = w!("HushTypeIndicator");
                let wc = WNDCLASSW { lpfnWndProc: Some(wndproc), hInstance: hinst.into(), lpszClassName: class, ..Default::default() };
                RegisterClassW(&wc);
                let hwnd = CreateWindowExW(
                    WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                    class,
                    w!("HushType"),
                    WS_POPUP,
                    0,
                    0,
                    1,
                    1,
                    None,
                    None,
                    Some(hinst.into()),
                    None,
                )
                .unwrap_or_default();
                let _ = tx.send(hwnd.0 as isize);
                let mut msg = MSG::default();
                while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            })
            .expect("spawn indicator thread");
        let hwnd = rx.recv().unwrap_or(0);
        Indicator { hwnd, inner }
    }

    pub fn set(&self, state: IndicatorState) {
        {
            let mut g = self.inner.lock().unwrap();
            if g.state == state {
                return;
            }
            g.state = state;
        }
        unsafe {
            let _ = PostMessageW(Some(HWND(self.hwnd as *mut _)), WM_UPDATE, WPARAM(0), LPARAM(0));
        }
    }

    /// Show near the top of the screen instead of the bottom.
    pub fn set_top(&self, top: bool) {
        self.inner.lock().unwrap().top = top;
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_UPDATE => {
            let (state, top) = match SHARED.get() {
                Some(s) => {
                    let g = s.lock().unwrap();
                    (g.state.clone(), g.top)
                }
                None => return LRESULT(0),
            };
            let _ = KillTimer(Some(hwnd), HIDE_TIMER);
            match &state {
                IndicatorState::Hidden => {
                    let _ = ShowWindow(hwnd, SW_HIDE);
                }
                other => {
                    render(hwnd, other, top);
                    let hide_after = match other {
                        IndicatorState::Success(_) => Some(1300),
                        IndicatorState::Error(_) => Some(5000),
                        IndicatorState::Info(_) => Some(2500),
                        _ => None,
                    };
                    if let Some(ms) = hide_after {
                        SetTimer(Some(hwnd), HIDE_TIMER, ms, None);
                    }
                }
            }
            LRESULT(0)
        }
        WM_TIMER if wparam.0 == HIDE_TIMER => {
            let _ = KillTimer(Some(hwnd), HIDE_TIMER);
            if let Some(s) = SHARED.get() {
                let mut g = s.lock().unwrap();
                if matches!(g.state, IndicatorState::Success(_) | IndicatorState::Error(_) | IndicatorState::Info(_)) {
                    g.state = IndicatorState::Hidden;
                    let _ = ShowWindow(hwnd, SW_HIDE);
                }
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF(r as u32 | (g as u32) << 8 | (b as u32) << 16)
}

unsafe fn font(height: i32, weight: i32, face: PCWSTR) -> HFONT {
    CreateFontW(
        -height,
        0,
        0,
        0,
        weight,
        0,
        0,
        0,
        DEFAULT_CHARSET,
        OUT_DEFAULT_PRECIS,
        CLIP_DEFAULT_PRECIS,
        CLEARTYPE_QUALITY,
        0,
        face,
    )
}

unsafe fn text_width(dc: HDC, s: &[u16]) -> i32 {
    let mut sz = SIZE::default();
    let _ = GetTextExtentPoint32W(dc, s, &mut sz);
    sz.cx
}

/// Keep the *end* of the text (latest words), prefixing an ellipsis.
unsafe fn fit_tail(dc: HDC, text: &str, max: i32) -> Vec<u16> {
    let full: Vec<u16> = text.encode_utf16().collect();
    if text_width(dc, &full) <= max {
        return full;
    }
    let chars: Vec<char> = text.chars().collect();
    let mut start = 0;
    while start < chars.len() {
        start += 1;
        let s: String = std::iter::once('…').chain(chars[start..].iter().copied()).collect();
        let u: Vec<u16> = s.encode_utf16().collect();
        if text_width(dc, &u) <= max {
            return u;
        }
    }
    "…".encode_utf16().collect()
}

/// Keep the *start* of the text, suffixing an ellipsis.
unsafe fn fit_head(dc: HDC, text: &str, max: i32) -> Vec<u16> {
    let full: Vec<u16> = text.encode_utf16().collect();
    if text_width(dc, &full) <= max {
        return full;
    }
    let chars: Vec<char> = text.chars().collect();
    let mut end = chars.len();
    while end > 0 {
        end -= 1;
        let s: String = chars[..end].iter().copied().chain(std::iter::once('…')).collect();
        let u: Vec<u16> = s.encode_utf16().collect();
        if text_width(dc, &u) <= max {
            return u;
        }
    }
    "…".encode_utf16().collect()
}

unsafe fn render(hwnd: HWND, state: &IndicatorState, top: bool) {
    let (glyph, glyph_color, label, detail): (&str, COLORREF, String, Option<String>) = match state {
        IndicatorState::Listening(None) => ("●", rgb(255, 77, 79), "Listening…".into(), None),
        IndicatorState::Listening(Some(t)) => ("●", rgb(255, 77, 79), "Listening".into(), Some(t.clone())),
        IndicatorState::Processing => ("●", rgb(245, 165, 36), "Processing…".into(), None),
        IndicatorState::Success(m) => ("✓", rgb(34, 197, 94), m.clone(), None),
        IndicatorState::Error(m) => ("⚠", rgb(251, 146, 60), m.clone(), None),
        IndicatorState::Info(m) => ("●", rgb(96, 165, 250), m.clone(), None),
        IndicatorState::Hidden => return,
    };

    // Monitor of the app the user is working in.
    let fg = GetForegroundWindow();
    let mon = MonitorFromWindow(if fg.is_invalid() { hwnd } else { fg }, MONITOR_DEFAULTTONEAREST);
    let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
    let _ = GetMonitorInfoW(mon, &mut mi);
    let work: RECT = mi.rcWork;
    let (mut dpi_x, mut dpi_y) = (96u32, 96u32);
    let _ = GetDpiForMonitor(mon, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y);
    let scale = dpi_x as f32 / 96.0;
    let px = |v: f32| (v * scale).round() as i32;

    let screen = GetDC(None);
    let dc = CreateCompatibleDC(Some(screen));
    let f_label = font(px(14.0), FW_SEMIBOLD.0 as i32, w!("Segoe UI"));
    let f_detail = font(px(14.0), FW_NORMAL.0 as i32, w!("Segoe UI"));
    let f_glyph = font(px(13.0), FW_NORMAL.0 as i32, w!("Segoe UI Symbol"));

    let h = px(38.0);
    let pad = px(16.0);
    let gap = px(8.0);
    let max_w = px(620.0).min(((work.right - work.left) as f32 * 0.8) as i32);

    let glyph_w16: Vec<u16> = glyph.encode_utf16().collect();
    let old = SelectObject(dc, HGDIOBJ(f_glyph.0));
    let glyph_w = text_width(dc, &glyph_w16);
    SelectObject(dc, HGDIOBJ(f_label.0));
    let label16 = fit_head(dc, &label, max_w - 2 * pad - glyph_w - gap);
    let label_w = text_width(dc, &label16);
    let mut w = pad + glyph_w + gap + label_w + pad;
    let mut detail16: Option<Vec<u16>> = None;
    if let Some(d) = &detail {
        SelectObject(dc, HGDIOBJ(f_detail.0));
        let avail = max_w - w - gap;
        if avail > px(60.0) {
            let d16 = fit_tail(dc, d, avail);
            w += gap + text_width(dc, &d16);
            detail16 = Some(d16);
        }
    }
    let w = w.max(h * 2);

    // 32-bit top-down DIB we can write alpha into.
    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
    let Ok(bmp) = CreateDIBSection(Some(dc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0) else {
        cleanup(dc, screen, &[f_label, f_detail, f_glyph], old);
        return;
    };
    let old_bmp = SelectObject(dc, HGDIOBJ(bmp.0));
    let n = (w * h) as usize;
    let pixels = std::slice::from_raw_parts_mut(bits as *mut u32, n);
    let (bg_r, bg_g, bg_b) = (30u32, 30u32, 34u32);
    pixels.fill(bg_r << 16 | bg_g << 8 | bg_b);

    SetBkMode(dc, TRANSPARENT);
    let mut x = pad;
    SelectObject(dc, HGDIOBJ(f_glyph.0));
    SetTextColor(dc, glyph_color);
    let _ = TextOutW(dc, x, (h - px(17.0)) / 2, &glyph_w16);
    x += glyph_w + gap;
    SelectObject(dc, HGDIOBJ(f_label.0));
    SetTextColor(dc, rgb(245, 245, 247));
    let _ = TextOutW(dc, x, (h - px(19.0)) / 2, &label16);
    x += label_w + gap;
    if let Some(d16) = &detail16 {
        SelectObject(dc, HGDIOBJ(f_detail.0));
        SetTextColor(dc, rgb(190, 190, 198));
        let _ = TextOutW(dc, x, (h - px(19.0)) / 2, d16);
    }

    // Antialiased pill shape with a faint border, premultiplied alpha.
    let r = h as f32 / 2.0;
    let opacity = 0.95f32;
    let (br, bg, bb) = (70.0f32, 70.0f32, 78.0f32);
    for y in 0..h {
        for xx in 0..w {
            let (cx, cy) = (xx as f32 + 0.5, y as f32 + 0.5);
            let dx = (r - cx).max(cx - (w as f32 - r)).max(0.0);
            let dy = (r - cy).max(cy - (h as f32 - r)).max(0.0);
            let d = (dx * dx + dy * dy).sqrt();
            let cov = (r - d + 0.5).clamp(0.0, 1.0);
            let i = (y * w + xx) as usize;
            if cov <= 0.0 {
                pixels[i] = 0;
                continue;
            }
            let p = pixels[i];
            let (mut pr, mut pg, mut pb) = (((p >> 16) & 0xFF) as f32, ((p >> 8) & 0xFF) as f32, (p & 0xFF) as f32);
            let edge = (r - d).clamp(0.0, 1.5);
            if edge < 1.5 {
                let t = 1.0 - edge / 1.5;
                pr += (br - pr) * t;
                pg += (bg - pg) * t;
                pb += (bb - pb) * t;
            }
            let a = cov * opacity;
            pixels[i] = ((a * 255.0) as u32) << 24 | ((pr * a) as u32) << 16 | ((pg * a) as u32) << 8 | (pb * a) as u32;
        }
    }

    let x_pos = work.left + ((work.right - work.left) - w) / 2;
    let y_pos = if top { work.top + px(18.0) } else { work.bottom - h - px(28.0) };
    let blend = BLENDFUNCTION { BlendOp: AC_SRC_OVER as u8, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: AC_SRC_ALPHA as u8 };
    let _ = UpdateLayeredWindow(
        hwnd,
        Some(screen),
        Some(&POINT { x: x_pos, y: y_pos }),
        Some(&SIZE { cx: w, cy: h }),
        Some(dc),
        Some(&POINT { x: 0, y: 0 }),
        COLORREF(0),
        Some(&blend),
        ULW_ALPHA,
    );
    let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW);
    let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);

    SelectObject(dc, old_bmp);
    let _ = DeleteObject(HGDIOBJ(bmp.0));
    cleanup(dc, screen, &[f_label, f_detail, f_glyph], old);
    let _ = wide; // keep helper import used on all cfgs
}

unsafe fn cleanup(dc: HDC, screen: HDC, fonts: &[HFONT], old: HGDIOBJ) {
    SelectObject(dc, old);
    for f in fonts {
        let _ = DeleteObject(HGDIOBJ(f.0));
    }
    let _ = DeleteDC(dc);
    ReleaseDC(None, screen);
}
