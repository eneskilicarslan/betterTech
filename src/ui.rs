use crate::config;
use crate::control_names::control_name;
use crate::device::{self, DeviceDescriptor, Snapshot};
use anyhow::{Result, anyhow};
use eframe::egui::{self, Color32, CornerRadius, FontId, Margin, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Vec2};
use hidapi::HidApi;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;
use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

// ============================================================================
// Memory Optimization (Zero-Bloat Physical Working Set Trimming)
// ============================================================================

#[cfg(windows)]
pub fn trim_working_set() {
    unsafe {
        unsafe extern "system" {
            fn SetProcessWorkingSetSize(
                h_process: isize,
                dw_minimum_working_set_size: usize,
                dw_maximum_working_set_size: usize,
            ) -> i32;
            fn K32EmptyWorkingSet(h_process: isize) -> i32;
            fn GetCurrentProcess() -> isize;
        }
        let proc = GetCurrentProcess();
        K32EmptyWorkingSet(proc);
        SetProcessWorkingSetSize(proc, usize::MAX, usize::MAX);
    }
}

#[cfg(not(windows))]
pub fn trim_working_set() {}

#[cfg(windows)]
mod win32_window {
    const GWL_STYLE: i32 = -16;
    const GWL_EXSTYLE: i32 = -20;
    const WS_OVERLAPPEDWINDOW: isize = 0x00CF_0000;
    const WS_EX_TOOLWINDOW: isize = 0x0000_0080;
    const WS_EX_APPWINDOW: isize = 0x0004_0000;
    const DWMWA_CLOAK: u32 = 13;
    const SW_HIDE: i32 = 0;
    const SW_SHOW: i32 = 5;
    const SW_RESTORE: i32 = 9;

    unsafe extern "system" {
        fn GetCurrentThreadId() -> u32;
        fn EnumThreadWindows(
            dw_thread_id: u32,
            lpfn: unsafe extern "system" fn(isize, isize) -> i32,
            l_param: isize,
        ) -> i32;
        fn GetWindowLongPtrW(h_wnd: isize, n_index: i32) -> isize;
        fn SetWindowLongPtrW(h_wnd: isize, n_index: i32, dw_new_long: isize) -> isize;
        fn ShowWindow(h_wnd: isize, n_cmd_show: i32) -> i32;
        fn SetForegroundWindow(h_wnd: isize) -> i32;
    }

    #[link(name = "dwmapi")]
    unsafe extern "system" {
        fn DwmSetWindowAttribute(
            hwnd: isize,
            dw_attribute: u32,
            pv_attribute: *const core::ffi::c_void,
            cb_attribute: u32,
        ) -> i32;
    }

    unsafe extern "system" fn enum_thread_wnd_proc(hwnd: isize, lparam: isize) -> i32 {
        unsafe {
            let style = GetWindowLongPtrW(hwnd, GWL_STYLE);
            if (style & WS_OVERLAPPEDWINDOW) != 0 {
                let out = lparam as *mut isize;
                *out = hwnd;
                return 0;
            }
            1
        }
    }

    pub fn find_main_hwnd() -> isize {
        let mut hwnd: isize = 0;
        unsafe {
            let tid = GetCurrentThreadId();
            EnumThreadWindows(tid, enum_thread_wnd_proc, (&raw mut hwnd) as isize);
        }
        hwnd
    }

    pub fn cloak_and_hide_from_taskbar(hwnd: isize) {
        if hwnd == 0 {
            return;
        }
        unsafe {
            let cloaked: i32 = 1;
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_CLOAK,
                (&raw const cloaked).cast(),
                core::mem::size_of::<i32>() as u32,
            );
            let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            let new_ex_style = (ex_style | WS_EX_TOOLWINDOW) & !WS_EX_APPWINDOW;
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, new_ex_style);
            ShowWindow(hwnd, SW_HIDE);
        }
    }

    pub fn uncloak_and_show_in_taskbar(hwnd: isize) {
        if hwnd == 0 {
            return;
        }
        unsafe {
            let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            let new_ex_style = (ex_style & !WS_EX_TOOLWINDOW) | WS_EX_APPWINDOW;
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, new_ex_style);
            let cloaked: i32 = 0;
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_CLOAK,
                (&raw const cloaked).cast(),
                core::mem::size_of::<i32>() as u32,
            );
        }
    }

    pub fn force_hide(hwnd: isize) {
        if hwnd == 0 {
            return;
        }
        unsafe {
            ShowWindow(hwnd, SW_HIDE);
        }
    }

    pub fn show_and_focus(hwnd: isize) {
        if hwnd == 0 {
            return;
        }
        unsafe {
            ShowWindow(hwnd, SW_RESTORE);
            ShowWindow(hwnd, SW_SHOW);
            SetForegroundWindow(hwnd);
        }
    }
}

#[cfg(not(windows))]
mod win32_window {
    pub fn find_main_hwnd() -> isize {
        0
    }
    pub fn cloak_and_hide_from_taskbar(_hwnd: isize) {}
    pub fn uncloak_and_show_in_taskbar(_hwnd: isize) {}
    pub fn force_hide(_hwnd: isize) {}
    pub fn show_and_focus(_hwnd: isize) {}
}

// ============================================================================
// Modern Obsidian / Zinc Color Palette
// ============================================================================

struct Theme;

impl Theme {
    const BG_CANVAS: Color32 = Color32::from_rgb(10, 12, 16);
    const BG_SIDEBAR: Color32 = Color32::from_rgb(14, 17, 23);
    const BG_TOPBAR: Color32 = Color32::from_rgb(14, 17, 23);
    const BG_CARD: Color32 = Color32::from_rgb(18, 22, 31);
    const BG_CARD_ALT: Color32 = Color32::from_rgb(15, 18, 26);
    const BG_ELEVATED: Color32 = Color32::from_rgb(24, 29, 40);
    const BG_HOVER: Color32 = Color32::from_rgb(29, 36, 50);
    const BG_SELECTED: Color32 = Color32::from_rgb(20, 38, 62);

    const BORDER_SUBTLE: Color32 = Color32::from_rgb(28, 35, 49);
    const BORDER_CARD: Color32 = Color32::from_rgb(35, 43, 60);
    const BORDER_HOVER: Color32 = Color32::from_rgb(56, 70, 96);

    const ACCENT: Color32 = Color32::from_rgb(14, 165, 233);
    const ACCENT_BRIGHT: Color32 = Color32::from_rgb(56, 189, 248);
    const ACCENT_DIM: Color32 = Color32::from_rgb(14, 42, 66);

    const SUCCESS: Color32 = Color32::from_rgb(16, 185, 129);
    const SUCCESS_BRIGHT: Color32 = Color32::from_rgb(52, 211, 153);
    const SUCCESS_DIM: Color32 = Color32::from_rgb(12, 40, 32);

    const WARNING: Color32 = Color32::from_rgb(245, 158, 11);
    const WARNING_DIM: Color32 = Color32::from_rgb(48, 32, 12);

    const DANGER: Color32 = Color32::from_rgb(239, 68, 68);
    const DANGER_BRIGHT: Color32 = Color32::from_rgb(248, 113, 113);
    const DANGER_DIM: Color32 = Color32::from_rgb(48, 18, 24);

    const PURPLE: Color32 = Color32::from_rgb(168, 85, 247);
    const PURPLE_DIM: Color32 = Color32::from_rgb(40, 22, 62);

    const TEXT_PRIMARY: Color32 = Color32::from_rgb(248, 250, 252);
    const TEXT_SECONDARY: Color32 = Color32::from_rgb(160, 174, 192);
    const TEXT_MUTED: Color32 = Color32::from_rgb(100, 116, 139);
}

// ============================================================================
// Procedural Vector Icons (Zero Texture Memory Overhead)
// ============================================================================

#[derive(Copy, Clone, PartialEq, Eq)]
enum IconKind {
    Mouse,
    Keyboard,
    Sun,
    Sliders,
    Keys,
    Hosts,
    Cpu,
    Refresh,
    Check,
    Settings,
    Tray,
    Bolt,
    Lock,
    Unlock,
    Reset,
}

fn draw_icon(painter: &egui::Painter, center: Pos2, size: f32, kind: IconKind, color: Color32) {
    let s = size * 0.5;
    let stroke = Stroke::new(1.5, color);

    match kind {
        IconKind::Mouse => {
            let body = Rect::from_center_size(center, Vec2::new(size * 0.68, size * 0.92));
            painter.rect_stroke(body, CornerRadius::same((size * 0.34) as u8), stroke, StrokeKind::Inside);
            painter.line_segment(
                [Pos2::new(center.x, body.min.y + 3.0), Pos2::new(center.x, center.y - 1.0)],
                stroke,
            );
        }
        IconKind::Keyboard => {
            let body = Rect::from_center_size(center, Vec2::new(size * 0.96, size * 0.66));
            painter.rect_stroke(body, CornerRadius::same(3), stroke, StrokeKind::Inside);
            let y1 = body.min.y + body.height() * 0.36;
            let y2 = body.min.y + body.height() * 0.68;
            for i in 0..3 {
                let x = body.min.x + body.width() * (0.25 + i as f32 * 0.25);
                painter.circle_filled(Pos2::new(x, y1), 1.1, color);
            }
            painter.line_segment(
                [
                    Pos2::new(body.min.x + body.width() * 0.28, y2),
                    Pos2::new(body.max.x - body.width() * 0.28, y2),
                ],
                Stroke::new(1.5, color),
            );
        }
        IconKind::Sun => {
            painter.circle_stroke(center, size * 0.22, stroke);
            for i in 0..8 {
                let angle = i as f32 * std::f32::consts::TAU / 8.0;
                let p1 = Pos2::new(center.x + angle.cos() * (s * 0.64), center.y + angle.sin() * (s * 0.64));
                let p2 = Pos2::new(center.x + angle.cos() * (s * 0.92), center.y + angle.sin() * (s * 0.92));
                painter.line_segment([p1, p2], Stroke::new(1.3, color));
            }
        }
        IconKind::Sliders => {
            for (idx, frac) in [0.3, 0.7, 0.45].iter().enumerate() {
                let y = center.y + (idx as f32 - 1.0) * (size * 0.30);
                let x0 = center.x - s * 0.85;
                let x1 = center.x + s * 0.85;
                painter.line_segment([Pos2::new(x0, y), Pos2::new(x1, y)], Stroke::new(1.3, color));
                let kx = x0 + (x1 - x0) * frac;
                painter.circle_filled(Pos2::new(kx, y), 2.2, color);
            }
        }
        IconKind::Keys => {
            let r1 = Rect::from_center_size(Pos2::new(center.x - s * 0.35, center.y - s * 0.2), Vec2::new(size * 0.42, size * 0.42));
            let r2 = Rect::from_center_size(Pos2::new(center.x + s * 0.35, center.y + s * 0.2), Vec2::new(size * 0.42, size * 0.42));
            painter.rect_stroke(r1, CornerRadius::same(2), stroke, StrokeKind::Inside);
            painter.rect_stroke(r2, CornerRadius::same(2), stroke, StrokeKind::Inside);
        }
        IconKind::Hosts => {
            let r = s * 0.72;
            let p1 = Pos2::new(center.x - r * 0.8, center.y + r * 0.6);
            let p2 = Pos2::new(center.x, center.y - r * 0.75);
            let p3 = Pos2::new(center.x + r * 0.8, center.y + r * 0.6);
            painter.line_segment([p1, p2], stroke);
            painter.line_segment([p2, p3], stroke);
            painter.circle_filled(p1, 2.2, color);
            painter.circle_filled(p2, 2.2, color);
            painter.circle_filled(p3, 2.2, color);
        }
        IconKind::Cpu => {
            let core = Rect::from_center_size(center, Vec2::splat(size * 0.56));
            painter.rect_stroke(core, CornerRadius::same(2), stroke, StrokeKind::Inside);
            for offset in [-0.35f32, 0.35f32] {
                let d = offset * size * 0.5;
                painter.line_segment([Pos2::new(center.x + d, core.min.y - 2.5), Pos2::new(center.x + d, core.min.y)], stroke);
                painter.line_segment([Pos2::new(center.x + d, core.max.y), Pos2::new(center.x + d, core.max.y + 2.5)], stroke);
                painter.line_segment([Pos2::new(core.min.x - 2.5, center.y + d), Pos2::new(core.min.x, center.y + d)], stroke);
                painter.line_segment([Pos2::new(core.max.x, center.y + d), Pos2::new(core.max.x + 2.5, center.y + d)], stroke);
            }
        }
        IconKind::Refresh => {
            let r = s * 0.72;
            let pts: Vec<Pos2> = (0..=14)
                .map(|i| {
                    let a = -0.6 + (i as f32 / 14.0) * 4.7;
                    Pos2::new(center.x + a.cos() * r, center.y + a.sin() * r)
                })
                .collect();
            for w in pts.windows(2) {
                painter.line_segment([w[0], w[1]], stroke);
            }
            let tip = Pos2::new(center.x + (-0.6f32).cos() * r, center.y + (-0.6f32).sin() * r);
            painter.line_segment([tip, Pos2::new(tip.x - 3.5, tip.y - 1.0)], stroke);
            painter.line_segment([tip, Pos2::new(tip.x - 0.5, tip.y + 3.5)], stroke);
        }
        IconKind::Check => {
            let p1 = Pos2::new(center.x - s * 0.65, center.y + s * 0.05);
            let p2 = Pos2::new(center.x - s * 0.15, center.y + s * 0.55);
            let p3 = Pos2::new(center.x + s * 0.72, center.y - s * 0.45);
            painter.line_segment([p1, p2], Stroke::new(1.8, color));
            painter.line_segment([p2, p3], Stroke::new(1.8, color));
        }
        IconKind::Settings => {
            painter.circle_stroke(center, s * 0.45, stroke);
            for i in 0..6 {
                let a = i as f32 * std::f32::consts::TAU / 6.0;
                let p1 = Pos2::new(center.x + a.cos() * (s * 0.50), center.y + a.sin() * (s * 0.50));
                let p2 = Pos2::new(center.x + a.cos() * (s * 0.85), center.y + a.sin() * (s * 0.85));
                painter.line_segment([p1, p2], Stroke::new(1.6, color));
            }
        }
        IconKind::Tray => {
            let tray = Rect::from_center_size(center, Vec2::new(size * 0.82, size * 0.72));
            painter.rect_stroke(tray, CornerRadius::same(2), stroke, StrokeKind::Inside);
            painter.line_segment(
                [Pos2::new(center.x, center.y - s * 0.35), Pos2::new(center.x, center.y + s * 0.35)],
                stroke,
            );
            painter.line_segment(
                [Pos2::new(center.x - 3.0, center.y + 0.5), Pos2::new(center.x, center.y + s * 0.35)],
                stroke,
            );
            painter.line_segment(
                [Pos2::new(center.x + 3.0, center.y + 0.5), Pos2::new(center.x, center.y + s * 0.35)],
                stroke,
            );
        }
        IconKind::Bolt => {
            let pts = vec![
                Pos2::new(center.x + 1.5, center.y - s * 0.88),
                Pos2::new(center.x - s * 0.58, center.y + 1.0),
                Pos2::new(center.x + 0.5, center.y + 1.0),
                Pos2::new(center.x - 1.5, center.y + s * 0.88),
                Pos2::new(center.x + s * 0.58, center.y - 1.0),
                Pos2::new(center.x - 0.5, center.y - 1.0),
            ];
            painter.add(egui::Shape::convex_polygon(pts, color, Stroke::NONE));
        }
        IconKind::Lock => {
            let body = Rect::from_center_size(Pos2::new(center.x, center.y + s * 0.24), Vec2::new(size * 0.68, size * 0.52));
            painter.rect_stroke(body, CornerRadius::same(2), stroke, StrokeKind::Inside);
            let shackle = Rect::from_min_max(
                Pos2::new(center.x - s * 0.36, center.y - s * 0.72),
                Pos2::new(center.x + s * 0.36, body.min.y + 1.0),
            );
            painter.rect_stroke(shackle, CornerRadius { nw: 5, ne: 5, sw: 0, se: 0 }, stroke, StrokeKind::Inside);
        }
        IconKind::Unlock => {
            let body = Rect::from_center_size(Pos2::new(center.x, center.y + s * 0.24), Vec2::new(size * 0.68, size * 0.52));
            painter.rect_stroke(body, CornerRadius::same(2), stroke, StrokeKind::Inside);
            painter.line_segment(
                [Pos2::new(center.x - s * 0.36, body.min.y), Pos2::new(center.x - s * 0.36, center.y - s * 0.6)],
                stroke,
            );
            painter.line_segment(
                [Pos2::new(center.x - s * 0.36, center.y - s * 0.6), Pos2::new(center.x + s * 0.32, center.y - s * 0.6)],
                stroke,
            );
        }
        IconKind::Reset => {
            let r = s * 0.68;
            painter.circle_stroke(center, r, stroke);
            painter.circle_filled(center, 1.8, color);
        }
    }
}

fn draw_battery_gauge(painter: &egui::Painter, rect: Rect, level: u8, charging: bool, color: Color32) {
    let body = Rect::from_min_max(rect.min, Pos2::new(rect.max.x - 3.0, rect.max.y));
    let tip = Rect::from_min_max(
        Pos2::new(body.max.x, body.center().y - 3.0),
        Pos2::new(rect.max.x, body.center().y + 3.0),
    );
    painter.rect_stroke(body, CornerRadius::same(3), Stroke::new(1.3, color), StrokeKind::Inside);
    painter.rect_filled(tip, CornerRadius::same(1), color);

    let inner = body.shrink(2.8);
    let fill_frac = (level.min(100) as f32 / 100.0).clamp(0.08, 1.0);
    let fill_rect = Rect::from_min_max(
        inner.min,
        Pos2::new(inner.min.x + inner.width() * fill_frac, inner.max.y),
    );
    painter.rect_filled(fill_rect, CornerRadius::same(1), color);

    if charging {
        draw_icon(painter, body.center(), 10.0, IconKind::Bolt, Color32::WHITE);
    }
}

// ============================================================================
// Reusable Modern UI Controls
// ============================================================================

fn toggle_switch(ui: &mut egui::Ui, on: bool) -> egui::Response {
    let desired_size = Vec2::new(40.0, 22.0);
    let (rect, response) = ui.allocate_exact_size(desired_size, Sense::click());
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if ui.is_rect_visible(rect) {
        let how_on = ui.ctx().animate_bool_responsive(response.id, on);
        let bg = Color32::from_rgb(
            (28.0 + (14.0 - 28.0) * how_on) as u8,
            (35.0 + (165.0 - 35.0) * how_on) as u8,
            (49.0 + (233.0 - 49.0) * how_on) as u8,
        );
        let border = if on {
            Theme::ACCENT_BRIGHT
        } else if response.hovered() {
            Theme::BORDER_HOVER
        } else {
            Theme::BORDER_CARD
        };
        let painter = ui.painter();
        painter.rect_filled(rect, CornerRadius::same(11), bg);
        painter.rect_stroke(rect, CornerRadius::same(11), Stroke::new(1.0, border), StrokeKind::Inside);
        let circle_x = egui::lerp((rect.left() + 11.0)..=(rect.right() - 11.0), how_on);
        let center = Pos2::new(circle_x, rect.center().y);
        painter.circle_filled(center, 7.5, Color32::WHITE);
    }
    response
}

fn toolbar_button(ui: &mut egui::Ui, icon: IconKind, label: &str, primary: bool) -> bool {
    let font = FontId::proportional(12.0);
    let text_galley = ui.painter().layout_no_wrap(label.to_owned(), font, Color32::WHITE);
    let width = text_galley.size().x + 34.0;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 30.0), Sense::click());
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if ui.is_rect_visible(rect) {
        let (bg, border, fg) = if primary {
            if response.hovered() {
                (Theme::ACCENT_BRIGHT, Theme::ACCENT_BRIGHT, Color32::from_rgb(7, 12, 20))
            } else {
                (Theme::ACCENT, Theme::ACCENT_BRIGHT, Color32::from_rgb(7, 12, 20))
            }
        } else if response.hovered() {
            (Theme::BG_HOVER, Theme::BORDER_HOVER, Theme::TEXT_PRIMARY)
        } else {
            (Theme::BG_ELEVATED, Theme::BORDER_CARD, Theme::TEXT_SECONDARY)
        };
        let painter = ui.painter();
        painter.rect_filled(rect, CornerRadius::same(7), bg);
        painter.rect_stroke(rect, CornerRadius::same(7), Stroke::new(1.0, border), StrokeKind::Inside);
        draw_icon(painter, Pos2::new(rect.min.x + 14.0, rect.center().y), 13.0, icon, fg);
        painter.text(
            Pos2::new(rect.min.x + 25.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            label,
            FontId::proportional(12.0),
            fg,
        );
    }
    response.clicked()
}

fn nav_tab_button(
    ui: &mut egui::Ui,
    current_tab: &mut DeviceTab,
    tab: DeviceTab,
    icon: IconKind,
    label: &str,
) {
    let is_active = *current_tab == tab;
    let font = FontId::proportional(12.5);
    let galley = ui.painter().layout_no_wrap(label.to_owned(), font, Color32::WHITE);
    let width = galley.size().x + 38.0;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 32.0), Sense::click());
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if response.clicked() {
        *current_tab = tab;
        trim_working_set();
    }
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let (bg, border, fg, icon_col) = if is_active {
            (Theme::BG_SELECTED, Theme::ACCENT, Theme::TEXT_PRIMARY, Theme::ACCENT_BRIGHT)
        } else if response.hovered() {
            (Theme::BG_HOVER, Theme::BORDER_SUBTLE, Theme::TEXT_PRIMARY, Theme::TEXT_SECONDARY)
        } else {
            (Color32::TRANSPARENT, Color32::TRANSPARENT, Theme::TEXT_SECONDARY, Theme::TEXT_MUTED)
        };
        painter.rect_filled(rect, CornerRadius::same(7), bg);
        if border != Color32::TRANSPARENT {
            painter.rect_stroke(rect, CornerRadius::same(7), Stroke::new(1.0, border), StrokeKind::Inside);
        }
        draw_icon(painter, Pos2::new(rect.min.x + 15.0, rect.center().y), 13.5, icon, icon_col);
        painter.text(
            Pos2::new(rect.min.x + 27.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            label,
            FontId::proportional(12.5),
            fg,
        );
    }
}

fn pill_badge(ui: &mut egui::Ui, text: &str, fg: Color32, bg: Color32) {
    egui::Frame::new()
        .fill(bg)
        .corner_radius(CornerRadius::same(5))
        .inner_margin(Margin::symmetric(7, 3))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(10.0).color(fg).strong());
        });
}

// ============================================================================
// Application Entry Point & Font / Visual Setup
// ============================================================================

pub fn run(api: HidApi, devices: Vec<DeviceDescriptor>, start_minimized: bool) -> Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("betterTech — Logitech Hardware Studio")
            .with_inner_size([1120.0, 740.0])
            .with_min_inner_size([880.0, 580.0])
            .with_visible(!start_minimized)
            .with_active(!start_minimized),
        ..Default::default()
    };
    eframe::run_native(
        "betterTech",
        options,
        Box::new(move |cc| {
            configure_fonts(&cc.egui_ctx);
            configure_visuals(&cc.egui_ctx);
            let main_hwnd = win32_window::find_main_hwnd();
            let tray = match build_tray() {
                Ok(tray) => Some(tray),
                Err(error) => {
                    eprintln!("tray icon unavailable: {error:#}");
                    None
                }
            };
            let hide_in_tray = start_minimized && tray.is_some();
            if hide_in_tray {
                // eframe unconditionally calls `window.set_visible(true)` after rendering
                // the first frame (`post_rendering`). Cloak the window and mark it as a
                // toolwindow beforehand so neither the window nor a taskbar icon flashes.
                win32_window::cloak_and_hide_from_taskbar(main_hwnd);
            }
            Ok(Box::new(App::new(
                api,
                devices,
                tray,
                cc.egui_ctx.clone(),
                main_hwnd,
                hide_in_tray,
            )))
        }),
    )
    .map_err(|error| anyhow!("running the GUI: {error}"))
}

fn configure_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    #[cfg(windows)]
    {
        if let Ok(bytes) = std::fs::read("C:\\Windows\\Fonts\\segoeui.ttf") {
            fonts.font_data.insert(
                "segoeui".to_owned(),
                std::sync::Arc::new(egui::FontData::from_owned(bytes)),
            );
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "segoeui".to_owned());
        }
        if let Ok(bytes) = std::fs::read("C:\\Windows\\Fonts\\seguisym.ttf") {
            fonts.font_data.insert(
                "seguisym".to_owned(),
                std::sync::Arc::new(egui::FontData::from_owned(bytes)),
            );
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .push("seguisym".to_owned());
        }
    }
    ctx.set_fonts(fonts);
}

fn configure_visuals(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.override_text_color = Some(Theme::TEXT_PRIMARY);
    visuals.panel_fill = Theme::BG_CANVAS;
    visuals.window_fill = Theme::BG_SIDEBAR;
    visuals.window_stroke = Stroke::new(1.0, Theme::BORDER_CARD);
    visuals.window_corner_radius = CornerRadius::same(12);
    visuals.extreme_bg_color = Color32::from_rgb(8, 10, 14);

    visuals.widgets.noninteractive.bg_fill = Theme::BG_CARD;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, Theme::BORDER_SUBTLE);
    visuals.widgets.noninteractive.corner_radius = CornerRadius::same(8);

    visuals.widgets.inactive.bg_fill = Theme::BG_ELEVATED;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, Theme::BORDER_CARD);
    visuals.widgets.inactive.corner_radius = CornerRadius::same(7);

    visuals.widgets.hovered.bg_fill = Theme::BG_HOVER;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, Theme::ACCENT);
    visuals.widgets.hovered.corner_radius = CornerRadius::same(7);

    visuals.widgets.active.bg_fill = Theme::ACCENT;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, Theme::ACCENT_BRIGHT);
    visuals.widgets.active.corner_radius = CornerRadius::same(7);

    visuals.selection.bg_fill = Theme::ACCENT;
    visuals.selection.stroke = Stroke::new(1.0, Color32::WHITE);

    ctx.set_visuals(visuals);
    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = Vec2::new(8.0, 8.0);
        style.spacing.button_padding = Vec2::new(10.0, 6.0);
        style.spacing.window_margin = Margin::same(16);
    });
}

// ============================================================================
// System Tray Integration
// ============================================================================

struct TrayParts {
    _icon: TrayIcon,
    show_item: MenuItem,
    quit_item: MenuItem,
}

fn build_tray() -> Result<TrayParts> {
    let show_item = MenuItem::new("Open betterTech", true, None);
    let quit_item = MenuItem::new("Quit", true, None);
    let menu = Menu::new();
    menu.append_items(&[&show_item, &quit_item])?;
    let icon = Icon::from_rgba(tray_rgba(), 32, 32)?;
    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("betterTech — Logitech Hardware Controller")
        .with_icon(icon)
        .with_menu_on_left_click(false)
        .build()?;
    Ok(TrayParts {
        _icon: tray,
        show_item,
        quit_item,
    })
}

fn tray_rgba() -> Vec<u8> {
    const SIZE: usize = 32;
    let mut rgba = vec![0u8; SIZE * SIZE * 4];
    let center = (SIZE as f32 - 1.0) / 2.0;
    for y in 0..SIZE {
        for x in 0..SIZE {
            let dx = x as f32 - center;
            let dy = y as f32 - center;
            let dist_sq = dx * dx + dy * dy;
            if dist_sq <= 14.0 * 14.0 {
                let offset = (y * SIZE + x) * 4;
                if dist_sq <= 6.0 * 6.0 {
                    rgba[offset] = 0x38;
                    rgba[offset + 1] = 0xBD;
                    rgba[offset + 2] = 0xF8;
                    rgba[offset + 3] = 0xFF;
                } else {
                    rgba[offset] = 0x0E;
                    rgba[offset + 1] = 0xA5;
                    rgba[offset + 2] = 0xE9;
                    rgba[offset + 3] = 0xFF;
                }
            }
        }
    }
    rgba
}

enum TrayEvent {
    Icon(TrayIconEvent),
    Menu(MenuEvent),
}

fn spawn_tray_waker(ctx: egui::Context, sender: Sender<TrayEvent>) {
    std::thread::spawn(move || {
        loop {
            let mut had_event = false;
            while let Ok(event) = TrayIconEvent::receiver().try_recv() {
                if matches!(
                    event,
                    TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } | TrayIconEvent::DoubleClick {
                        button: MouseButton::Left,
                        ..
                    }
                ) {
                    had_event = true;
                    let _ = sender.send(TrayEvent::Icon(event));
                }
            }
            while let Ok(event) = MenuEvent::receiver().try_recv() {
                had_event = true;
                let _ = sender.send(TrayEvent::Menu(event));
            }
            if had_event {
                ctx.request_repaint();
            }
            std::thread::sleep(Duration::from_millis(120));
        }
    });
}

// ============================================================================
// Navigation & Hardware Worker Types
// ============================================================================

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum DeviceTab {
    MouseButtons,
    SensorAndPower,
    ControlsRemap,
    Backlight,
    KeyboardSettings,
    EasySwitch,
    Diagnostics,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum MousePresetFilter {
    Keys,
    All,
    Mouse,
    DpiSpecial,
    Shortcuts,
    FunctionNav,
    Media,
}

struct BacklightEffectPreset {
    id: u8,
    name: &'static str,
    desc: &'static str,
}

const BACKLIGHT_EFFECTS: [BacklightEffectPreset; 6] = [
    BacklightEffectPreset {
        id: 0,
        name: "Static",
        desc: "Continuous, even illumination across all keys.",
    },
    BacklightEffectPreset {
        id: 1,
        name: "Contrast",
        desc: "Modifier keys glow brighter than alphanumeric keys.",
    },
    BacklightEffectPreset {
        id: 2,
        name: "Breathing",
        desc: "Smooth, rhythmic fade in and out cycle.",
    },
    BacklightEffectPreset {
        id: 3,
        name: "Waves",
        desc: "Undulating light wave sweeping left to right.",
    },
    BacklightEffectPreset {
        id: 4,
        name: "Reaction",
        desc: "Keys flare up dynamically as you type.",
    },
    BacklightEffectPreset {
        id: 5,
        name: "Random",
        desc: "Starlight pattern of softly twinkling keys.",
    },
];

enum DeviceAction {
    SetBacklight {
        descriptor: DeviceDescriptor,
        level: u8,
        manual: bool,
        enabled: bool,
        effect: u8,
        duration: u16,
    },
    SetBacklightEffect {
        descriptor: DeviceDescriptor,
        effect: u8,
    },
    SetPollingRate {
        descriptor: DeviceDescriptor,
        rate_hz: u16,
    },
    SetPowerMode {
        descriptor: DeviceDescriptor,
        mode: u8,
    },
    SetFnInversion {
        descriptor: DeviceDescriptor,
        inverted: bool,
    },
    SetDisabledKeys {
        descriptor: DeviceDescriptor,
        mask: u8,
    },
    SetDpiStage {
        descriptor: DeviceDescriptor,
        stage: u8,
    },
    SetMouseButton {
        descriptor: DeviceDescriptor,
        button_index: u8,
        raw: [u8; 4],
    },
    ResetAllMouseButtons {
        descriptor: DeviceDescriptor,
    },
    SetMapping {
        descriptor: DeviceDescriptor,
        from: u16,
        to: u16,
    },
    SwitchHost {
        descriptor: DeviceDescriptor,
        host: u8,
    },
}

fn spawn_hardware_worker() -> Sender<DeviceAction> {
    let (sender, receiver) = mpsc::channel::<DeviceAction>();
    std::thread::spawn(move || {
        let Ok(api) = HidApi::new() else {
            eprintln!("Hardware worker failed to initialize HidApi");
            return;
        };
        while let Ok(action) = receiver.recv() {
            match action {
                DeviceAction::SetBacklight {
                    descriptor,
                    level,
                    manual,
                    enabled,
                    effect,
                    duration,
                } => {
                    let _ = device::set_backlight_full(
                        &api,
                        &descriptor,
                        level,
                        manual,
                        enabled,
                        effect,
                        duration,
                    );
                }
                DeviceAction::SetBacklightEffect { descriptor, effect } => {
                    let _ = device::set_backlight_effect(&api, &descriptor, effect);
                }
                DeviceAction::SetPollingRate { descriptor, rate_hz } => {
                    let _ = device::set_polling_rate(&api, &descriptor, rate_hz);
                }
                DeviceAction::SetPowerMode { descriptor, mode } => {
                    let _ = device::set_power_mode(&api, &descriptor, mode);
                }
                DeviceAction::SetFnInversion { descriptor, inverted } => {
                    let _ = device::set_fn_inversion(&api, &descriptor, inverted);
                }
                DeviceAction::SetDisabledKeys { descriptor, mask } => {
                    let _ = device::set_disabled_keys(&api, &descriptor, mask);
                }
                DeviceAction::SetDpiStage { descriptor, stage } => {
                    let _ = device::set_dpi_stage(&api, &descriptor, stage);
                }
                DeviceAction::SetMouseButton {
                    descriptor,
                    button_index,
                    raw,
                } => {
                    let _ = device::set_mouse_button(&api, &descriptor, button_index, raw);
                }
                DeviceAction::ResetAllMouseButtons { descriptor } => {
                    let defaults: [[u8; 4]; 6] = [
                        [0x80, 0x01, 0x00, 0x01],
                        [0x80, 0x01, 0x00, 0x02],
                        [0x80, 0x01, 0x00, 0x04],
                        [0x80, 0x01, 0x00, 0x08],
                        [0x80, 0x01, 0x00, 0x10],
                        [0x90, 0x05, 0x00, 0x00],
                    ];
                    for (i, def) in defaults.iter().enumerate() {
                        let _ = device::set_mouse_button(&api, &descriptor, i as u8, *def);
                    }
                }
                DeviceAction::SetMapping { descriptor, from, to } => {
                    let _ = device::set_mapping(&api, &descriptor, from, to);
                }
                DeviceAction::SwitchHost { descriptor, host } => {
                    let _ = device::switch_host(&api, &descriptor, host);
                }
            }
        }
    });
    sender
}

// ============================================================================
// Application State
// ============================================================================

fn has_missing_configured_devices(devices: &[DeviceDescriptor]) -> bool {
    if devices.is_empty() {
        return true;
    }
    let Ok(cfg) = config::load() else {
        return false;
    };
    let settings_missing = cfg.device_settings.iter().any(|s| {
        !s.product_id.is_empty()
            && !devices
                .iter()
                .any(|d| d.product_id.eq_ignore_ascii_case(&s.product_id))
    });
    let mappings_missing = cfg.mappings.iter().any(|m| {
        !m.product_id.is_empty()
            && !devices
                .iter()
                .any(|d| d.product_id.eq_ignore_ascii_case(&m.product_id))
    });
    settings_missing || mappings_missing
}

fn spawn_startup_discovery(
    ctx: egui::Context,
    initial_devices: Vec<DeviceDescriptor>,
) -> Receiver<(Vec<DeviceDescriptor>, bool)> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut known_devices = initial_devices;
        // Only probe on initial startup if no devices responded yet (up to 5 tries).
        let mut startup_probes_remaining: u8 = if known_devices.is_empty() { 5 } else { 0 };
        let mut resume_probes_remaining: u8 = 0;
        let mut last_wall = std::time::SystemTime::now();

        loop {
            let sleep_secs = if startup_probes_remaining > 0 || resume_probes_remaining > 0 {
                4
            } else {
                5
            };
            std::thread::sleep(Duration::from_secs(sleep_secs));

            let now_wall = std::time::SystemTime::now();
            if let Ok(elapsed) = now_wall.duration_since(last_wall) {
                if elapsed > Duration::from_secs(sleep_secs + 8) {
                    // System resumed from Windows Sleep / Hibernate: probe up to 4 times
                    resume_probes_remaining = 4;
                }
            }
            last_wall = now_wall;

            if startup_probes_remaining == 0 && resume_probes_remaining == 0 {
                continue;
            }

            let is_resume = resume_probes_remaining > 0;
            if startup_probes_remaining > 0 {
                startup_probes_remaining -= 1;
            }
            if resume_probes_remaining > 0 {
                resume_probes_remaining -= 1;
            }

            let Ok(api) = HidApi::new() else {
                continue;
            };
            let Ok(found) = device::discover(&api) else {
                continue;
            };
            if !found.is_empty() && (is_resume || found.len() > known_devices.len()) {
                known_devices = found.clone();
                startup_probes_remaining = 0;
                resume_probes_remaining = 0;
                if tx.send((found, true)).is_err() {
                    break;
                }
                ctx.request_repaint();
            }
        }
    });
    rx
}

struct App {
    api: HidApi,
    devices: Vec<DeviceDescriptor>,
    selected: usize,
    snapshot: Option<Snapshot>,
    status: String,
    status_is_error: bool,
    search_query: String,
    feature_search: String,
    filter_remapped_only: bool,
    current_tab: DeviceTab,
    backlight_duration_secs: u16,
    backlight_battery_saving: bool,
    action_sender: Sender<DeviceAction>,
    tray: Option<TrayParts>,
    tray_events: Receiver<TrayEvent>,
    discovery_events: Receiver<(Vec<DeviceDescriptor>, bool)>,
    show_settings_modal: bool,
    autostart_enabled: bool,
    is_quitting: bool,
    main_hwnd: isize,
    hide_on_startup_frames: u8,
    window_cloaked: bool,
    startup_trims_remaining: u8,
    selected_mouse_button: u8,
    mouse_preset_filter: MousePresetFilter,
    recording_mouse_button: Option<u8>,
}

impl App {
    fn new(
        api: HidApi,
        devices: Vec<DeviceDescriptor>,
        tray: Option<TrayParts>,
        ctx: egui::Context,
        main_hwnd: isize,
        start_minimized: bool,
    ) -> Self {
        let (sender, tray_events) = mpsc::channel();
        if tray.is_some() {
            spawn_tray_waker(ctx.clone(), sender);
        }
        let discovery_events = spawn_startup_discovery(ctx, devices.clone());
        let action_sender = spawn_hardware_worker();
        let mut app = Self {
            api,
            devices,
            selected: 0,
            snapshot: None,
            status: "Ready".into(),
            status_is_error: false,
            search_query: String::new(),
            feature_search: String::new(),
            filter_remapped_only: false,
            current_tab: DeviceTab::ControlsRemap,
            backlight_duration_secs: 30,
            backlight_battery_saving: true,
            action_sender,
            tray,
            tray_events,
            discovery_events,
            show_settings_modal: false,
            autostart_enabled: crate::autostart::is_enabled(),
            is_quitting: false,
            main_hwnd,
            hide_on_startup_frames: if start_minimized { 2 } else { 0 },
            window_cloaked: start_minimized,
            startup_trims_remaining: 3,
            selected_mouse_button: 0,
            mouse_preset_filter: MousePresetFilter::Keys,
            recording_mouse_button: None,
        };
        if !app.devices.is_empty() {
            app.apply_saved();
        }
        app
    }

    fn show_from_tray(&mut self, ctx: &egui::Context) {
        self.hide_on_startup_frames = 0;
        if self.window_cloaked {
            win32_window::uncloak_and_show_in_taskbar(self.main_hwnd);
            self.window_cloaked = false;
        }
        if has_missing_configured_devices(&self.devices) {
            let prev_len = self.devices.len();
            self.reload_devices();
            if self.devices.len() > prev_len {
                self.apply_saved();
            }
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        win32_window::show_and_focus(self.main_hwnd);
    }

    fn hide_to_tray(&mut self, ctx: &egui::Context) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        win32_window::force_hide(self.main_hwnd);
        trim_working_set();
    }

    fn set_status(&mut self, msg: &str, is_error: bool) {
        self.status = msg.to_string();
        self.status_is_error = is_error;
    }

    fn reload_devices(&mut self) {
        let _ = self.api.refresh_devices();
        match device::discover(&self.api) {
            Ok(devices) => {
                self.devices = devices;
                self.selected = self.selected.min(self.devices.len().saturating_sub(1));
                self.snapshot = None;
                self.set_status(&format!("Discovered {} Logitech device(s)", self.devices.len()), false);
                trim_working_set();
            }
            Err(error) => self.set_status(&format!("Discovery failed: {error:#}"), true),
        }
    }

    fn reload_snapshot(&mut self) {
        if self.devices.is_empty() {
            self.snapshot = None;
            return;
        }
        let descriptor = &self.devices[self.selected];
        match device::snapshot(&self.api, descriptor) {
            Ok(snapshot) => {
                let has_mouse_buttons = snapshot.mouse_buttons.is_some();
                let has_power = snapshot.polling_rate_hz.is_some() || snapshot.mode_status.is_some() || snapshot.dpi.is_some();
                let has_controls = !snapshot.controls.is_empty();
                let has_backlight = snapshot.backlight.is_some();
                let has_kb_settings = snapshot.fn_inverted.is_some() || snapshot.disabled_keys.is_some();
                let has_hosts = snapshot.hosts.is_some();

                let tab_valid = match self.current_tab {
                    DeviceTab::MouseButtons => has_mouse_buttons,
                    DeviceTab::SensorAndPower => has_power,
                    DeviceTab::ControlsRemap => has_controls,
                    DeviceTab::Backlight => has_backlight,
                    DeviceTab::KeyboardSettings => has_kb_settings,
                    DeviceTab::EasySwitch => has_hosts,
                    DeviceTab::Diagnostics => true,
                };

                if !tab_valid {
                    if has_mouse_buttons {
                        self.current_tab = DeviceTab::MouseButtons;
                    } else if has_controls {
                        self.current_tab = DeviceTab::ControlsRemap;
                    } else if has_backlight {
                        self.current_tab = DeviceTab::Backlight;
                    } else if has_power {
                        self.current_tab = DeviceTab::SensorAndPower;
                    } else {
                        self.current_tab = DeviceTab::Diagnostics;
                    }
                }

                if let Some(backlight) = &snapshot.backlight {
                    self.backlight_duration_secs = backlight.dpow;
                }
                if let Ok(cfg) = crate::config::load() {
                    if let Some(s) = cfg.device_settings.iter().find(|s| {
                        s.product_id == descriptor.product_id && s.unit_id == descriptor.unit_id
                    }) {
                        if let Some(saving) = s.backlight_battery_saving {
                            self.backlight_battery_saving = saving;
                        }
                        if let Some(dur) = s.backlight_duration {
                            self.backlight_duration_secs = dur;
                        }
                    }
                }
                self.snapshot = Some(snapshot);
                trim_working_set();
            }
            Err(error) => {
                self.set_status(&format!("Reading {} failed: {error:#}", descriptor.name), true);
                self.snapshot = None;
            }
        }
    }

    fn apply_mapping(&mut self, from: u16, to: u16) {
        let Some(descriptor) = self.devices.get(self.selected).cloned() else {
            return;
        };
        let from_name = control_name(from);
        let to_name = control_name(to);
        if let Some(snapshot) = self.snapshot.as_mut() {
            if let Some(ctrl) = snapshot.controls.iter_mut().find(|c| c.cid == from) {
                ctrl.mapped_to = to;
                ctrl.mapped_to_name = to_name.clone();
            }
        }
        if from == to {
            let _ = config::remove(&descriptor.product_id, &descriptor.unit_id, from);
            self.set_status(&format!("Restored '{from_name}' to default action"), false);
        } else {
            let _ = config::record(
                &descriptor.product_id,
                &descriptor.unit_id,
                from,
                to,
                &from_name,
                &to_name,
            );
            self.set_status(&format!("Remapped '{from_name}' → '{to_name}'"), false);
        }
        let _ = self.action_sender.send(DeviceAction::SetMapping { descriptor, from, to });
    }

    fn reset_all_mappings(&mut self) {
        let Some(descriptor) = self.devices.get(self.selected).cloned() else {
            return;
        };
        let mut reset_cids = Vec::new();
        if let Some(snapshot) = self.snapshot.as_mut() {
            for ctrl in &mut snapshot.controls {
                if ctrl.mapped_to != ctrl.cid {
                    ctrl.mapped_to = ctrl.cid;
                    ctrl.mapped_to_name = ctrl.name.clone();
                    reset_cids.push(ctrl.cid);
                }
            }
        }
        for cid in reset_cids {
            let _ = config::remove(&descriptor.product_id, &descriptor.unit_id, cid);
            let _ = self.action_sender.send(DeviceAction::SetMapping {
                descriptor: descriptor.clone(),
                from: cid,
                to: cid,
            });
        }
        self.set_status("All reprogrammable keys restored to factory defaults", false);
    }

    fn set_polling_rate(&mut self, rate_hz: u16) {
        let Some(descriptor) = self.devices.get(self.selected).cloned() else {
            return;
        };
        if let Some(snapshot) = self.snapshot.as_mut() {
            snapshot.polling_rate_hz = Some(rate_hz);
        }
        let _ = config::update_device_settings(&descriptor.product_id, &descriptor.unit_id, |s| {
            s.polling_rate_hz = Some(rate_hz);
        });
        self.set_status(&format!("Polling rate set to {rate_hz} Hz"), false);
        let _ = self.action_sender.send(DeviceAction::SetPollingRate { descriptor, rate_hz });
    }

    fn set_power_mode(&mut self, mode: u8) {
        let Some(descriptor) = self.devices.get(self.selected).cloned() else {
            return;
        };
        if let Some(snapshot) = self.snapshot.as_mut() {
            snapshot.mode_status = Some(mode);
        }
        let _ = config::update_device_settings(&descriptor.product_id, &descriptor.unit_id, |s| {
            s.power_mode = Some(mode);
        });
        let mode_name = if mode == 1 {
            "Performance (1000 Hz)"
        } else {
            "Endurance (125 Hz Eco)"
        };
        self.set_status(&format!("Power mode switched to {mode_name}"), false);
        let _ = self.action_sender.send(DeviceAction::SetPowerMode { descriptor, mode });
    }

    fn set_backlight_level(&mut self, level: u8, manual: bool, enabled: bool) {
        let Some(descriptor) = self.devices.get(self.selected).cloned() else {
            return;
        };
        let (effect, duration) = if let Some(snapshot) = self.snapshot.as_mut() {
            if let Some(b) = snapshot.backlight.as_mut() {
                b.level = level;
                b.manual_mode = manual;
                b.enabled = enabled;
                (b.current_effect, b.dpow)
            } else {
                (0, self.backlight_duration_secs)
            }
        } else {
            (0, self.backlight_duration_secs)
        };
        let _ = config::update_device_settings(&descriptor.product_id, &descriptor.unit_id, |s| {
            s.backlight_level = Some(level);
            s.backlight_manual = Some(manual);
            s.backlight_enabled = Some(enabled);
        });
        let mode_text = if !enabled {
            "Off".to_string()
        } else if manual {
            format!("Manual Level {level}")
        } else {
            "Smart Ambient Auto".to_string()
        };
        self.set_status(&format!("Backlight updated: {mode_text}"), false);
        let _ = self.action_sender.send(DeviceAction::SetBacklight {
            descriptor,
            level,
            manual,
            enabled,
            effect,
            duration,
        });
    }

    fn set_backlight_full(&mut self, level: u8, manual: bool, enabled: bool, effect: u8, duration: u16) {
        let Some(descriptor) = self.devices.get(self.selected).cloned() else {
            return;
        };
        self.backlight_duration_secs = duration;
        if let Some(snapshot) = self.snapshot.as_mut() {
            if let Some(b) = snapshot.backlight.as_mut() {
                b.level = level;
                b.manual_mode = manual;
                b.enabled = enabled;
                if effect <= 5 {
                    b.current_effect = effect;
                }
                b.dpow = duration;
            }
        }
        let _ = config::update_device_settings(&descriptor.product_id, &descriptor.unit_id, |s| {
            s.backlight_level = Some(level);
            s.backlight_manual = Some(manual);
            s.backlight_enabled = Some(enabled);
            if effect <= 5 {
                s.backlight_effect = Some(effect);
            }
            s.backlight_duration = Some(duration);
            s.backlight_battery_saving = Some(self.backlight_battery_saving);
        });
        let _ = self.action_sender.send(DeviceAction::SetBacklight {
            descriptor,
            level,
            manual,
            enabled,
            effect,
            duration,
        });
    }

    fn set_backlight_effect(&mut self, effect: u8) {
        let Some(descriptor) = self.devices.get(self.selected).cloned() else {
            return;
        };
        if let Some(snapshot) = self.snapshot.as_mut() {
            if let Some(b) = snapshot.backlight.as_mut() {
                b.current_effect = effect;
            }
        }
        let _ = config::update_device_settings(&descriptor.product_id, &descriptor.unit_id, |s| {
            s.backlight_effect = Some(effect);
        });
        let eff_name = BACKLIGHT_EFFECTS.get(effect as usize).map(|e| e.name).unwrap_or("Custom");
        self.set_status(&format!("Backlight effect set to {eff_name}"), false);
        let _ = self.action_sender.send(DeviceAction::SetBacklightEffect { descriptor, effect });
    }

    fn set_backlight_duration(&mut self, duration: u16) {
        let Some(descriptor) = self.devices.get(self.selected).cloned() else {
            return;
        };
        self.backlight_duration_secs = duration;
        let (level, manual, enabled, effect) = if let Some(snapshot) = self.snapshot.as_mut() {
            if let Some(b) = snapshot.backlight.as_mut() {
                b.dho = duration;
                b.dhi = duration;
                b.dpow = duration;
                (b.level, b.manual_mode, b.enabled, b.current_effect)
            } else {
                (7, true, true, 0)
            }
        } else {
            (7, true, true, 0)
        };
        let _ = config::update_device_settings(&descriptor.product_id, &descriptor.unit_id, |s| {
            s.backlight_level = Some(level);
            s.backlight_manual = Some(manual);
            s.backlight_enabled = Some(enabled);
            s.backlight_duration = Some(duration);
        });
        self.set_status(&format!("Backlight sleep timeout set to {duration}s"), false);
        let _ = self.action_sender.send(DeviceAction::SetBacklight {
            descriptor,
            level,
            manual,
            enabled,
            effect,
            duration,
        });
    }

    fn set_backlight_battery_saving(&mut self, enabled: bool) {
        self.backlight_battery_saving = enabled;
        if let Some(descriptor) = self.devices.get(self.selected) {
            let _ = config::update_device_settings(&descriptor.product_id, &descriptor.unit_id, |s| {
                s.backlight_battery_saving = Some(enabled);
            });
        }
        let status = if enabled { "enabled (<10% auto-off)" } else { "disabled" };
        self.set_status(&format!("Battery saver {status}"), false);
    }

    fn reset_backlighting(&mut self) {
        self.backlight_battery_saving = true;
        self.set_backlight_full(4, false, true, 0, 30);
        self.set_status("Backlighting restored to factory defaults (Static, Auto, 30s)", false);
    }

    fn set_fn_inversion(&mut self, inverted: bool) {
        let Some(descriptor) = self.devices.get(self.selected).cloned() else {
            return;
        };
        if let Some(snapshot) = self.snapshot.as_mut() {
            snapshot.fn_inverted = Some(inverted);
        }
        let _ = config::update_device_settings(&descriptor.product_id, &descriptor.unit_id, |s| {
            s.fn_inverted = Some(inverted);
        });
        let mode = if inverted {
            "Standard F1–F12 primary"
        } else {
            "Media & Hotkeys primary"
        };
        self.set_status(&format!("Fn-Lock updated: {mode}"), false);
        let _ = self.action_sender.send(DeviceAction::SetFnInversion { descriptor, inverted });
    }

    fn toggle_disabled_key(&mut self, bit: u8) {
        let Some(descriptor) = self.devices.get(self.selected).cloned() else {
            return;
        };
        let current_mask = self
            .snapshot
            .as_ref()
            .and_then(|s| s.disabled_keys.as_ref())
            .map(|k| k.mask)
            .unwrap_or(0);
        let new_mask = current_mask ^ (1 << bit);
        if let Some(snapshot) = self.snapshot.as_mut() {
            if let Some(dis) = snapshot.disabled_keys.as_mut() {
                dis.mask = new_mask;
                dis.caps_lock = new_mask & 0x01 != 0;
                dis.scroll_lock = new_mask & 0x02 != 0;
                dis.num_lock = new_mask & 0x04 != 0;
                dis.win_key = new_mask & 0x08 != 0;
                dis.insert = new_mask & 0x10 != 0;
            }
        }
        let _ = config::update_device_settings(&descriptor.product_id, &descriptor.unit_id, |s| {
            s.disabled_keys_mask = Some(new_mask);
        });
        self.set_status("Updated hardware key lock mask", false);
        let _ = self.action_sender.send(DeviceAction::SetDisabledKeys {
            descriptor,
            mask: new_mask,
        });
    }

    fn set_dpi_stage(&mut self, stage_index: usize) {
        let Some(descriptor) = self.devices.get(self.selected).cloned() else {
            return;
        };
        let mut dpi_val = None;
        if let Some(snapshot) = self.snapshot.as_mut() {
            if let Some(dpi) = snapshot.dpi.as_mut() {
                dpi.active_index = stage_index;
                dpi_val = dpi.levels.get(stage_index).copied();
            }
        }
        let _ = config::update_device_settings(&descriptor.product_id, &descriptor.unit_id, |s| {
            s.dpi_stage = Some(stage_index as u8);
        });
        if let Some(val) = dpi_val {
            self.set_status(&format!("Switched to Stage {} ({val} DPI)", stage_index + 1), false);
        } else {
            self.set_status(&format!("Switched to DPI Stage {}", stage_index + 1), false);
        }
        let _ = self.action_sender.send(DeviceAction::SetDpiStage {
            descriptor,
            stage: stage_index as u8,
        });
    }

    fn switch_host(&mut self, host: u8) {
        let Some(descriptor) = self.devices.get(self.selected).cloned() else {
            return;
        };
        self.set_status(&format!("Switching device to Host Channel {}", host + 1), false);
        let _ = self.action_sender.send(DeviceAction::SwitchHost { descriptor, host });
    }

    fn set_mouse_button(&mut self, button_index: u8, raw: [u8; 4], label: &str) {
        let Some(descriptor) = self.devices.get(self.selected).cloned() else {
            return;
        };
        if let Some(snapshot) = self.snapshot.as_mut() {
            if let Some(buttons) = snapshot.mouse_buttons.as_mut() {
                if let Some(btn) = buttons.get_mut(button_index as usize) {
                    btn.raw = raw;
                    let (parsed_label, parsed_cat) = crate::hidpp::parse_mouse_binding(raw);
                    btn.binding_label = if label.is_empty() { parsed_label } else { label.to_string() };
                    btn.category = parsed_cat;
                }
            }
        }
        let _ = config::update_device_settings(&descriptor.product_id, &descriptor.unit_id, |s| {
            s.mouse_button_remaps.retain(|r| r.button_index != button_index);
            s.mouse_button_remaps.push(config::SavedMouseButtonRemap {
                button_index,
                raw,
                label: label.to_string(),
            });
        });
        self.set_status(&format!("Button {} mapped to '{label}'", button_index + 1), false);
        let _ = self.action_sender.send(DeviceAction::SetMouseButton {
            descriptor,
            button_index,
            raw,
        });
    }

    fn reset_all_mouse_buttons(&mut self) {
        let defaults: [[u8; 4]; 6] = [
            [0x80, 0x01, 0x00, 0x01],
            [0x80, 0x01, 0x00, 0x02],
            [0x80, 0x01, 0x00, 0x04],
            [0x80, 0x01, 0x00, 0x08],
            [0x80, 0x01, 0x00, 0x10],
            [0x90, 0x05, 0x00, 0x00],
        ];
        let default_labels = ["Left Click", "Right Click", "Middle Click", "Back", "Forward", "DPI Cycle"];
        let default_cats = ["Mouse", "Mouse", "Mouse", "Mouse", "Mouse", "DPI / Special"];
        let Some(descriptor) = self.devices.get(self.selected).cloned() else {
            return;
        };
        if let Some(snapshot) = self.snapshot.as_mut() {
            if let Some(buttons) = snapshot.mouse_buttons.as_mut() {
                for (i, btn) in buttons.iter_mut().enumerate() {
                    if i < 6 {
                        btn.raw = defaults[i];
                        btn.binding_label = default_labels[i].to_string();
                        btn.category = default_cats[i].to_string();
                    }
                }
            }
        }
        let _ = config::update_device_settings(&descriptor.product_id, &descriptor.unit_id, |s| {
            s.mouse_button_remaps.clear();
        });
        self.set_status("All 6 mouse buttons restored to factory defaults", false);
        let _ = self.action_sender.send(DeviceAction::ResetAllMouseButtons { descriptor });
    }

    fn apply_saved(&mut self) {
        let config = match config::load() {
            Ok(config) => config,
            Err(error) => {
                self.set_status(&format!("Reading config failed: {error:#}"), true);
                return;
            }
        };

        let mut applied_remaps = 0usize;
        for mapping in &config.mappings {
            let descriptor = self.devices.iter().find(|device| {
                device.product_id.eq_ignore_ascii_case(&mapping.product_id)
                    && (mapping.unit_id.is_empty()
                        || device.unit_id.eq_ignore_ascii_case(&mapping.unit_id))
            });
            if let Some(desc) = descriptor {
                if let Ok(true) = device::set_mapping(&self.api, desc, mapping.from, mapping.to) {
                    applied_remaps += 1;
                }
            }
        }

        let mut applied_settings = 0usize;
        for settings in &config.device_settings {
            let descriptor = self.devices.iter().find(|device| {
                device.product_id.eq_ignore_ascii_case(&settings.product_id)
                    && (settings.unit_id.is_empty()
                        || device.unit_id.eq_ignore_ascii_case(&settings.unit_id))
            });
            let Some(desc) = descriptor else { continue };

            if let Some(rate) = settings.polling_rate_hz {
                if let Ok(true) = device::set_polling_rate(&self.api, desc, rate) {
                    applied_settings += 1;
                }
            }
            if let Some(mode) = settings.power_mode {
                if let Ok(true) = device::set_power_mode(&self.api, desc, mode) {
                    applied_settings += 1;
                }
            }
            if let Some(stage) = settings.dpi_stage {
                if let Ok(true) = device::set_dpi_stage(&self.api, desc, stage) {
                    applied_settings += 1;
                }
            }
            if let Some(fn_inv) = settings.fn_inverted {
                if let Ok(true) = device::set_fn_inversion(&self.api, desc, fn_inv) {
                    applied_settings += 1;
                }
            }
            if settings.backlight_level.is_some()
                || settings.backlight_duration.is_some()
                || settings.backlight_manual.is_some()
                || settings.backlight_enabled.is_some()
            {
                let level = settings.backlight_level.unwrap_or(5);
                let effect = settings.backlight_effect.unwrap_or(0xFF);
                let duration = settings.backlight_duration.unwrap_or(30);
                if let Ok(true) = device::set_backlight_full(
                    &self.api,
                    desc,
                    level,
                    settings.backlight_manual.unwrap_or(false),
                    settings.backlight_enabled.unwrap_or(true),
                    effect,
                    duration,
                ) {
                    applied_settings += 1;
                }
            } else if let Some(effect) = settings.backlight_effect {
                if let Ok(true) = device::set_backlight_effect(&self.api, desc, effect) {
                    applied_settings += 1;
                }
            }
            if let Some(mask) = settings.disabled_keys_mask {
                if let Ok(true) = device::set_disabled_keys(&self.api, desc, mask) {
                    applied_settings += 1;
                }
            }
            for remap in &settings.mouse_button_remaps {
                if let Ok(true) = device::set_mouse_button(&self.api, desc, remap.button_index, remap.raw) {
                    applied_settings += 1;
                }
            }
        }

        self.set_status(
            &format!("Synced {applied_remaps} remap(s) and {applied_settings} hardware setting(s)"),
            false,
        );
        if self.window_cloaked {
            self.snapshot = None;
            trim_working_set();
        } else {
            self.reload_snapshot();
        }
    }
}

// ============================================================================
// Main eframe::App Implementation
// ============================================================================

fn egui_key_to_hid(key: egui::Key) -> Option<u8> {
    use egui::Key;
    match key {
        Key::A => Some(0x04),
        Key::B => Some(0x05),
        Key::C => Some(0x06),
        Key::D => Some(0x07),
        Key::E => Some(0x08),
        Key::F => Some(0x09),
        Key::G => Some(0x0A),
        Key::H => Some(0x0B),
        Key::I => Some(0x0C),
        Key::J => Some(0x0D),
        Key::K => Some(0x0E),
        Key::L => Some(0x0F),
        Key::M => Some(0x10),
        Key::N => Some(0x11),
        Key::O => Some(0x12),
        Key::P => Some(0x13),
        Key::Q => Some(0x14),
        Key::R => Some(0x15),
        Key::S => Some(0x16),
        Key::T => Some(0x17),
        Key::U => Some(0x18),
        Key::V => Some(0x19),
        Key::W => Some(0x1A),
        Key::X => Some(0x1B),
        Key::Y => Some(0x1C),
        Key::Z => Some(0x1D),
        Key::Num1 => Some(0x1E),
        Key::Num2 => Some(0x1F),
        Key::Num3 => Some(0x20),
        Key::Num4 => Some(0x21),
        Key::Num5 => Some(0x22),
        Key::Num6 => Some(0x23),
        Key::Num7 => Some(0x24),
        Key::Num8 => Some(0x25),
        Key::Num9 => Some(0x26),
        Key::Num0 => Some(0x27),
        Key::Enter => Some(0x28),
        Key::Escape => Some(0x29),
        Key::Backspace => Some(0x2A),
        Key::Tab => Some(0x2B),
        Key::Space => Some(0x2C),
        Key::Minus => Some(0x2D),
        Key::Equals => Some(0x2E),
        Key::OpenBracket => Some(0x2F),
        Key::CloseBracket => Some(0x30),
        Key::Backslash => Some(0x31),
        Key::Semicolon => Some(0x33),
        Key::Quote => Some(0x34),
        Key::Backtick => Some(0x35),
        Key::Comma => Some(0x36),
        Key::Period => Some(0x37),
        Key::Slash => Some(0x38),
        Key::F1 => Some(0x3A),
        Key::F2 => Some(0x3B),
        Key::F3 => Some(0x3C),
        Key::F4 => Some(0x3D),
        Key::F5 => Some(0x3E),
        Key::F6 => Some(0x3F),
        Key::F7 => Some(0x40),
        Key::F8 => Some(0x41),
        Key::F9 => Some(0x42),
        Key::F10 => Some(0x43),
        Key::F11 => Some(0x44),
        Key::F12 => Some(0x45),
        Key::Insert => Some(0x49),
        Key::Home => Some(0x4A),
        Key::PageUp => Some(0x4B),
        Key::Delete => Some(0x4C),
        Key::End => Some(0x4D),
        Key::PageDown => Some(0x4E),
        Key::ArrowRight => Some(0x4F),
        Key::ArrowLeft => Some(0x50),
        Key::ArrowDown => Some(0x51),
        Key::ArrowUp => Some(0x52),
        _ => None,
    }
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.hide_on_startup_frames > 0 {
            if self.hide_on_startup_frames == 2 {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            }
            self.hide_on_startup_frames -= 1;
            win32_window::force_hide(self.main_hwnd);
            trim_working_set();
        }

        if ctx.input(|i| i.viewport().close_requested()) {
            if self.tray.is_some() && !self.is_quitting {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.hide_to_tray(ctx);
            }
        }

        // Capture live keyboard press when recording a mouse button binding
        if let Some(btn_idx) = self.recording_mouse_button {
            let mut captured: Option<(u8, u8)> = None;
            ctx.input(|i| {
                for event in &i.events {
                    if let egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } = event
                    {
                        if let Some(hid_code) = egui_key_to_hid(*key) {
                            let mut mods = 0u8;
                            if modifiers.ctrl || modifiers.command {
                                mods |= 0x01;
                            }
                            if modifiers.shift {
                                mods |= 0x02;
                            }
                            if modifiers.alt {
                                mods |= 0x04;
                            }
                            captured = Some((mods, hid_code));
                            break;
                        }
                    }
                }
            });
            if let Some((mods, hid_code)) = captured {
                self.recording_mouse_button = None;
                let raw = [0x80, 0x02, mods, hid_code];
                let (label, _) = crate::hidpp::parse_mouse_binding(raw);
                self.set_mouse_button(btn_idx, raw, &label);
            }
        }

        while let Ok((discovered, sync_saved)) = self.discovery_events.try_recv() {
            if !discovered.is_empty() && (sync_saved || discovered.len() > self.devices.len()) {
                self.devices = discovered;
                self.selected = self.selected.min(self.devices.len().saturating_sub(1));
                self.snapshot = None;
                if sync_saved {
                    self.apply_saved();
                } else {
                    self.set_status(
                        &format!("Discovered {} Logitech device(s)", self.devices.len()),
                        false,
                    );
                }
                trim_working_set();
            }
        }

        while let Ok(event) = self.tray_events.try_recv() {
            match event {
                TrayEvent::Icon(TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                })
                | TrayEvent::Icon(TrayIconEvent::DoubleClick {
                    button: MouseButton::Left,
                    ..
                }) => self.show_from_tray(ctx),
                TrayEvent::Icon(_) => {}
                TrayEvent::Menu(event) => {
                    let Some(tray) = &self.tray else {
                        continue;
                    };
                    if event.id == *tray.show_item.id() {
                        self.show_from_tray(ctx);
                    } else if event.id == *tray.quit_item.id() {
                        self.is_quitting = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                }
            }
        }

        // Initial working-set trim on startup without continuous idle wakeups
        if self.startup_trims_remaining > 0 {
            self.startup_trims_remaining -= 1;
            trim_working_set();
            if self.startup_trims_remaining > 0 {
                ctx.request_repaint_after(Duration::from_millis(350));
            }
        }

        // Battery saving auto-off enforcement (< 10%)
        if self.backlight_battery_saving {
            if let Some(snapshot) = &self.snapshot {
                if let (Some(battery), Some(backlight)) = (&snapshot.battery, &snapshot.backlight) {
                    if let Some(level) = battery.level {
                        if level < 10 && backlight.enabled {
                            self.set_backlight_level(backlight.level, backlight.manual_mode, false);
                            self.set_status("Battery low (<10%): Backlight auto-disabled to save power", false);
                        }
                    }
                }
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if self.snapshot.is_none() && !self.devices.is_empty() {
            self.reload_snapshot();
        }

        // 1. Sleek Top Application Bar
        egui::Panel::top("top_panel")
            .frame(
                egui::Frame::new()
                    .fill(Theme::BG_TOPBAR)
                    .stroke(Stroke::new(1.0, Theme::BORDER_SUBTLE))
                    .inner_margin(Margin::symmetric(18, 10)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    // Brand mark + Title
                    let (logo_rect, _) = ui.allocate_exact_size(Vec2::new(26.0, 26.0), Sense::hover());
                    ui.painter().rect_filled(logo_rect, CornerRadius::same(7), Theme::ACCENT_DIM);
                    ui.painter().rect_stroke(
                        logo_rect,
                        CornerRadius::same(7),
                        Stroke::new(1.0, Theme::ACCENT),
                        StrokeKind::Inside,
                    );
                    draw_icon(ui.painter(), logo_rect.center(), 14.0, IconKind::Bolt, Theme::ACCENT_BRIGHT);

                    ui.add_space(4.0);
                    ui.label(
                        RichText::new("betterTech")
                            .color(Theme::TEXT_PRIMARY)
                            .size(16.0)
                            .strong(),
                    );
                    pill_badge(
                        ui,
                        concat!("v", env!("CARGO_PKG_VERSION")),
                        Theme::TEXT_SECONDARY,
                        Theme::BG_ELEVATED,
                    );

                    ui.add_space(12.0);

                    // Subtle Status Toast Pill
                    let (dot_col, bg_col, txt_col) = if self.status_is_error {
                        (Theme::DANGER, Theme::DANGER_DIM, Theme::DANGER_BRIGHT)
                    } else {
                        (Theme::SUCCESS, Theme::BG_CARD, Theme::TEXT_SECONDARY)
                    };
                    egui::Frame::new()
                        .fill(bg_col)
                        .stroke(Stroke::new(1.0, Theme::BORDER_SUBTLE))
                        .corner_radius(CornerRadius::same(14))
                        .inner_margin(Margin::symmetric(11, 4))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let (dot_r, _) = ui.allocate_exact_size(Vec2::splat(7.0), Sense::hover());
                                ui.painter().circle_filled(dot_r.center(), 3.2, dot_col);
                                ui.label(RichText::new(&self.status).size(11.5).color(txt_col));
                            });
                        });

                    // Right-aligned global actions
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if self.tray.is_some() && toolbar_button(ui, IconKind::Tray, "Minimize", false) {
                            let ctx = ui.ctx().clone();
                            self.hide_to_tray(&ctx);
                        }
                        if toolbar_button(ui, IconKind::Settings, "Preferences", false) {
                            self.show_settings_modal = !self.show_settings_modal;
                        }
                        if toolbar_button(ui, IconKind::Check, "Sync Profile", true) {
                            self.apply_saved();
                        }
                        if toolbar_button(ui, IconKind::Refresh, "Rescan", false) {
                            self.reload_devices();
                        }
                    });
                });
            });

        // 2. Left Sidebar: Connected Peripherals
        egui::Panel::left("devices_panel")
            .resizable(false)
            .default_size(248.0)
            .frame(
                egui::Frame::new()
                    .fill(Theme::BG_SIDEBAR)
                    .stroke(Stroke::new(1.0, Theme::BORDER_SUBTLE))
                    .inner_margin(Margin::symmetric(14, 16)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("PERIPHERALS")
                            .size(10.5)
                            .color(Theme::TEXT_MUTED)
                            .strong(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        pill_badge(
                            ui,
                            &format!("{} ONLINE", self.devices.len()),
                            Theme::ACCENT_BRIGHT,
                            Theme::ACCENT_DIM,
                        );
                    });
                });
                ui.add_space(10.0);

                let mut clicked_device = None;
                for (index, descriptor) in self.devices.iter().enumerate() {
                    let is_selected = self.selected == index;
                    let name_lower = descriptor.name.to_lowercase();
                    let is_keyboard = name_lower.contains("keyboard")
                        || name_lower.contains("mechanical")
                        || name_lower.contains("craft")
                        || name_lower.contains("keys");
                    let icon = if is_keyboard { IconKind::Keyboard } else { IconKind::Mouse };
                    let kind_label = if is_keyboard { "Keyboard" } else { "Mouse" };

                    let desired = Vec2::new(ui.available_width(), 64.0);
                    let (rect, resp) = ui.allocate_exact_size(desired, Sense::click());
                    if resp.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    if resp.clicked() && !is_selected {
                        clicked_device = Some(index);
                    }

                    if ui.is_rect_visible(rect) {
                        let painter = ui.painter();
                        let bg = if is_selected {
                            Theme::BG_SELECTED
                        } else if resp.hovered() {
                            Theme::BG_HOVER
                        } else {
                            Theme::BG_CARD
                        };
                        let border = if is_selected {
                            Stroke::new(1.5, Theme::ACCENT)
                        } else if resp.hovered() {
                            Stroke::new(1.0, Theme::BORDER_HOVER)
                        } else {
                            Stroke::new(1.0, Theme::BORDER_SUBTLE)
                        };

                        painter.rect_filled(rect, CornerRadius::same(10), bg);
                        painter.rect_stroke(rect, CornerRadius::same(10), border, StrokeKind::Inside);

                        if is_selected {
                            let bar = Rect::from_min_size(
                                Pos2::new(rect.min.x + 1.0, rect.min.y + 12.0),
                                Vec2::new(3.5, rect.height() - 24.0),
                            );
                            painter.rect_filled(bar, CornerRadius::same(2), Theme::ACCENT_BRIGHT);
                        }

                        // Icon container
                        let icon_box = Rect::from_center_size(
                            Pos2::new(rect.min.x + 28.0, rect.center().y),
                            Vec2::splat(34.0),
                        );
                        painter.rect_filled(
                            icon_box,
                            CornerRadius::same(8),
                            if is_selected { Theme::ACCENT_DIM } else { Theme::BG_ELEVATED },
                        );
                        draw_icon(
                            painter,
                            icon_box.center(),
                            17.0,
                            icon,
                            if is_selected { Theme::ACCENT_BRIGHT } else { Theme::TEXT_SECONDARY },
                        );

                        // Device Name & Meta
                        let text_x = icon_box.max.x + 11.0;
                        painter.text(
                            Pos2::new(text_x, rect.min.y + 21.0),
                            egui::Align2::LEFT_CENTER,
                            &descriptor.name,
                            FontId::proportional(13.5),
                            if is_selected { Theme::TEXT_PRIMARY } else { Theme::TEXT_SECONDARY },
                        );

                        // Status dot + subtitle
                        let dot_pos = Pos2::new(text_x + 4.0, rect.min.y + 42.0);
                        painter.circle_filled(dot_pos, 3.0, Theme::SUCCESS);
                        let meta = if descriptor.transport.is_empty() {
                            format!("{kind_label} • 0x{}", descriptor.product_id)
                        } else {
                            format!("{} • 0x{}", descriptor.transport, descriptor.product_id)
                        };
                        painter.text(
                            Pos2::new(text_x + 12.0, rect.min.y + 42.0),
                            egui::Align2::LEFT_CENTER,
                            meta,
                            FontId::proportional(11.0),
                            Theme::TEXT_MUTED,
                        );
                    }
                    ui.add_space(6.0);
                }

                if let Some(idx) = clicked_device {
                    self.selected = idx;
                    self.snapshot = None;
                    self.selected_mouse_button = 0;
                }

                if self.devices.is_empty() {
                    egui::Frame::new()
                        .fill(Theme::BG_CARD)
                        .stroke(Stroke::new(1.0, Theme::BORDER_SUBTLE))
                        .corner_radius(CornerRadius::same(10))
                        .inner_margin(Margin::same(14))
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new("No devices found")
                                    .color(Theme::TEXT_PRIMARY)
                                    .strong(),
                            );
                            ui.add_space(4.0);
                            ui.label(
                                RichText::new("Connect a Logitech USB receiver, cable, or Bluetooth device and click Rescan.")
                                    .size(11.5)
                                    .color(Theme::TEXT_MUTED),
                            );
                        });
                }

                // Bottom footer inside sidebar
                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    if let Ok(path) = config::path() {
                        ui.label(
                            RichText::new(path.display().to_string())
                                .size(9.5)
                                .color(Theme::TEXT_MUTED),
                        );
                        ui.label(
                            RichText::new("LOCAL PROFILE STORAGE")
                                .size(9.5)
                                .color(Theme::TEXT_MUTED)
                                .strong(),
                        );
                    }
                    ui.add_space(6.0);
                    ui.separator();
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        pill_badge(ui, "HID++ 2.0", Theme::SUCCESS_BRIGHT, Theme::SUCCESS_DIM);
                        pill_badge(ui, "Zero Telemetry", Theme::TEXT_SECONDARY, Theme::BG_ELEVATED);
                    });
                });
            });

        // 3. Central Workspace
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(Theme::BG_CANVAS)
                    .inner_margin(Margin::symmetric(22, 18)),
            )
            .show(ui, |ui| {
                if self.devices.is_empty() {
                    ui.centered_and_justified(|ui| {
                        ui.label(
                            RichText::new("Connect a Logitech HID++ peripheral to begin configuring hardware.")
                                .color(Theme::TEXT_MUTED)
                                .size(15.0),
                        );
                    });
                    return;
                }

                let Some(snapshot) = self.snapshot.clone() else {
                    ui.centered_and_justified(|ui| {
                        ui.label(
                            RichText::new("Querying device microcontroller...")
                                .size(15.0)
                                .color(Theme::ACCENT_BRIGHT),
                        );
                    });
                    return;
                };

                self.render_device_header(ui, &snapshot);
                ui.add_space(14.0);

                // Main Tab Content Area
                match self.current_tab {
                    DeviceTab::MouseButtons => self.render_mouse_buttons_tab(ui, &snapshot),
                    DeviceTab::SensorAndPower => self.render_sensor_and_power_tab(ui, &snapshot),
                    DeviceTab::ControlsRemap => self.render_controls_tab(ui, &snapshot),
                    DeviceTab::Backlight => self.render_backlight_tab(ui, &snapshot),
                    DeviceTab::KeyboardSettings => self.render_keyboard_settings_tab(ui, &snapshot),
                    DeviceTab::EasySwitch => self.render_easy_switch_tab(ui, &snapshot),
                    DeviceTab::Diagnostics => self.render_diagnostics_tab(ui, &snapshot),
                }
            });

        if self.show_settings_modal {
            self.render_settings_window(ui.ctx());
        }
    }
}

// ============================================================================
// Device Header & Navigation Bar
// ============================================================================

impl App {
    fn render_device_header(&mut self, ui: &mut egui::Ui, snapshot: &Snapshot) {
        let name_lower = snapshot.device.name.to_lowercase();
        let is_kb = name_lower.contains("keyboard")
            || name_lower.contains("mechanical")
            || name_lower.contains("craft")
            || name_lower.contains("keys");
        let icon = if is_kb { IconKind::Keyboard } else { IconKind::Mouse };
        let category_label = if is_kb {
            "WIRELESS MECHANICAL KEYBOARD"
        } else {
            "LIGHTSPEED WIRELESS GAMING MOUSE"
        };

        egui::Frame::new()
            .fill(Theme::BG_CARD)
            .stroke(Stroke::new(1.0, Theme::BORDER_SUBTLE))
            .corner_radius(CornerRadius::same(12))
            .inner_margin(Margin::symmetric(18, 14))
            .show(ui, |ui| {
                // Top Row: Device Identity + Live Telemetry Badges
                ui.horizontal(|ui| {
                    let (icon_rect, _) = ui.allocate_exact_size(Vec2::splat(46.0), Sense::hover());
                    ui.painter().rect_filled(icon_rect, CornerRadius::same(10), Theme::BG_ELEVATED);
                    ui.painter().rect_stroke(
                        icon_rect,
                        CornerRadius::same(10),
                        Stroke::new(1.0, Theme::BORDER_CARD),
                        StrokeKind::Inside,
                    );
                    draw_icon(ui.painter(), icon_rect.center(), 23.0, icon, Theme::ACCENT_BRIGHT);

                    ui.add_space(8.0);

                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new(category_label)
                                .size(10.0)
                                .color(Theme::ACCENT_BRIGHT)
                                .strong(),
                        );
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(&snapshot.device.name)
                                    .size(20.0)
                                    .strong()
                                    .color(Theme::TEXT_PRIMARY),
                            );
                            if !snapshot.device.transport.is_empty() {
                                pill_badge(
                                    ui,
                                    &snapshot.device.transport,
                                    Theme::TEXT_SECONDARY,
                                    Theme::BG_ELEVATED,
                                );
                            }
                        });
                    });

                    // Right Telemetry Readouts
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Battery Pill with Vector Gauge
                        if let Some(battery) = &snapshot.battery {
                            let lvl = battery.level.unwrap_or(50);
                            let charging = battery.status.contains("recharg") || battery.status.contains("charg");
                            let (bat_col, bat_bg) = if charging {
                                (Theme::ACCENT_BRIGHT, Theme::ACCENT_DIM)
                            } else if lvl > 35 {
                                (Theme::SUCCESS_BRIGHT, Theme::SUCCESS_DIM)
                            } else if lvl > 15 {
                                (Theme::WARNING, Theme::WARNING_DIM)
                            } else {
                                (Theme::DANGER_BRIGHT, Theme::DANGER_DIM)
                            };

                            egui::Frame::new()
                                .fill(bat_bg)
                                .stroke(Stroke::new(1.0, bat_col.gamma_multiply(0.45)))
                                .corner_radius(CornerRadius::same(8))
                                .inner_margin(Margin::symmetric(11, 6))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        let (g_rect, _) = ui.allocate_exact_size(Vec2::new(22.0, 12.0), Sense::hover());
                                        draw_battery_gauge(ui.painter(), g_rect, lvl, charging, bat_col);
                                        ui.label(
                                            RichText::new(format!("{lvl}%"))
                                                .size(13.0)
                                                .strong()
                                                .color(bat_col),
                                        );
                                        if let Some(mv) = battery.voltage_mv {
                                            ui.label(
                                                RichText::new(format!("{mv} mV"))
                                                    .size(11.0)
                                                    .color(Theme::TEXT_SECONDARY),
                                            );
                                        }
                                    });
                                });
                        }

                        // Active Polling Rate Pill
                        if let Some(hz) = snapshot.polling_rate_hz {
                            egui::Frame::new()
                                .fill(Theme::BG_ELEVATED)
                                .stroke(Stroke::new(1.0, Theme::BORDER_CARD))
                                .corner_radius(CornerRadius::same(8))
                                .inner_margin(Margin::symmetric(10, 6))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        let (r, _) = ui.allocate_exact_size(Vec2::splat(12.0), Sense::hover());
                                        draw_icon(ui.painter(), r.center(), 12.0, IconKind::Bolt, Theme::ACCENT_BRIGHT);
                                        ui.label(
                                            RichText::new(format!("{hz} Hz"))
                                                .size(12.5)
                                                .strong()
                                                .color(Theme::TEXT_PRIMARY),
                                        );
                                    });
                                });
                        }

                        // Active DPI Stage Pill
                        if let Some(dpi) = &snapshot.dpi {
                            if let Some(active_dpi) = dpi.levels.get(dpi.active_index) {
                                egui::Frame::new()
                                    .fill(Theme::BG_ELEVATED)
                                    .stroke(Stroke::new(1.0, Theme::BORDER_CARD))
                                    .corner_radius(CornerRadius::same(8))
                                    .inner_margin(Margin::symmetric(10, 6))
                                    .show(ui, |ui| {
                                        ui.label(
                                            RichText::new(format!("{active_dpi} DPI"))
                                                .size(12.5)
                                                .strong()
                                                .color(Theme::ACCENT_BRIGHT),
                                        );
                                    });
                            }
                        }
                    });
                });

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(8.0);

                // Bottom Row inside Header: Contextual Navigation Tabs
                ui.horizontal(|ui| {
                    let has_mouse_buttons = snapshot.mouse_buttons.is_some();
                    let has_power = snapshot.polling_rate_hz.is_some()
                        || snapshot.mode_status.is_some()
                        || snapshot.dpi.is_some();
                    let has_controls = !snapshot.controls.is_empty();
                    let has_backlight = snapshot.backlight.is_some();
                    let has_keyboard_settings =
                        snapshot.fn_inverted.is_some() || snapshot.disabled_keys.is_some();
                    let has_hosts = snapshot.hosts.is_some();

                    if has_mouse_buttons {
                        nav_tab_button(
                            ui,
                            &mut self.current_tab,
                            DeviceTab::MouseButtons,
                            IconKind::Mouse,
                            "Buttons & Actions",
                        );
                    }
                    if has_power {
                        nav_tab_button(
                            ui,
                            &mut self.current_tab,
                            DeviceTab::SensorAndPower,
                            IconKind::Sliders,
                            "Sensor & Performance",
                        );
                    }
                    if has_controls {
                        nav_tab_button(
                            ui,
                            &mut self.current_tab,
                            DeviceTab::ControlsRemap,
                            IconKind::Keys,
                            "Key Remapping",
                        );
                    }
                    if has_backlight {
                        nav_tab_button(
                            ui,
                            &mut self.current_tab,
                            DeviceTab::Backlight,
                            IconKind::Sun,
                            "Illumination",
                        );
                    }
                    if has_keyboard_settings {
                        nav_tab_button(
                            ui,
                            &mut self.current_tab,
                            DeviceTab::KeyboardSettings,
                            IconKind::Sliders,
                            "Keyboard Mode",
                        );
                    }
                    if has_hosts {
                        nav_tab_button(
                            ui,
                            &mut self.current_tab,
                            DeviceTab::EasySwitch,
                            IconKind::Hosts,
                            "Easy-Switch",
                        );
                    }
                    nav_tab_button(
                        ui,
                        &mut self.current_tab,
                        DeviceTab::Diagnostics,
                        IconKind::Cpu,
                        "Hardware & Features",
                    );
                });
            });
    }
}

// ============================================================================
// Tab 1: Mouse Buttons & Interactive Ergonomic Blueprint
// ============================================================================

#[derive(Clone, Copy)]
struct MousePreset {
    name: &'static str,
    category: &'static str,
    raw: [u8; 4],
}

const MOUSE_PRESETS: &[MousePreset] = &[
    // Mouse
    MousePreset { name: "Left Click", category: "Mouse", raw: [0x80, 0x01, 0x00, 0x01] },
    MousePreset { name: "Right Click", category: "Mouse", raw: [0x80, 0x01, 0x00, 0x02] },
    MousePreset { name: "Middle Click", category: "Mouse", raw: [0x80, 0x01, 0x00, 0x04] },
    MousePreset { name: "Back (Side 1)", category: "Mouse", raw: [0x80, 0x01, 0x00, 0x08] },
    MousePreset { name: "Forward (Side 2)", category: "Mouse", raw: [0x80, 0x01, 0x00, 0x10] },
    // Keyboard Keys (A-Z & 0-9)
    MousePreset { name: "A", category: "Keys (A–Z / 0–9)", raw: [0x80, 0x02, 0x00, 0x04] },
    MousePreset { name: "B", category: "Keys (A–Z / 0–9)", raw: [0x80, 0x02, 0x00, 0x05] },
    MousePreset { name: "C", category: "Keys (A–Z / 0–9)", raw: [0x80, 0x02, 0x00, 0x06] },
    MousePreset { name: "D", category: "Keys (A–Z / 0–9)", raw: [0x80, 0x02, 0x00, 0x07] },
    MousePreset { name: "E", category: "Keys (A–Z / 0–9)", raw: [0x80, 0x02, 0x00, 0x08] },
    MousePreset { name: "F", category: "Keys (A–Z / 0–9)", raw: [0x80, 0x02, 0x00, 0x09] },
    MousePreset { name: "G", category: "Keys (A–Z / 0–9)", raw: [0x80, 0x02, 0x00, 0x0A] },
    MousePreset { name: "H", category: "Keys (A–Z / 0–9)", raw: [0x80, 0x02, 0x00, 0x0B] },
    MousePreset { name: "I", category: "Keys (A–Z / 0–9)", raw: [0x80, 0x02, 0x00, 0x0C] },
    MousePreset { name: "J", category: "Keys (A–Z / 0–9)", raw: [0x80, 0x02, 0x00, 0x0D] },
    MousePreset { name: "K", category: "Keys (A–Z / 0–9)", raw: [0x80, 0x02, 0x00, 0x0E] },
    MousePreset { name: "L", category: "Keys (A–Z / 0–9)", raw: [0x80, 0x02, 0x00, 0x0F] },
    MousePreset { name: "M", category: "Keys (A–Z / 0–9)", raw: [0x80, 0x02, 0x00, 0x10] },
    MousePreset { name: "N", category: "Keys (A–Z / 0–9)", raw: [0x80, 0x02, 0x00, 0x11] },
    MousePreset { name: "O", category: "Keys (A–Z / 0–9)", raw: [0x80, 0x02, 0x00, 0x12] },
    MousePreset { name: "P", category: "Keys (A–Z / 0–9)", raw: [0x80, 0x02, 0x00, 0x13] },
    MousePreset { name: "Q", category: "Keys (A–Z / 0–9)", raw: [0x80, 0x02, 0x00, 0x14] },
    MousePreset { name: "R", category: "Keys (A–Z / 0–9)", raw: [0x80, 0x02, 0x00, 0x15] },
    MousePreset { name: "S", category: "Keys (A–Z / 0–9)", raw: [0x80, 0x02, 0x00, 0x16] },
    MousePreset { name: "T", category: "Keys (A–Z / 0–9)", raw: [0x80, 0x02, 0x00, 0x17] },
    MousePreset { name: "U", category: "Keys (A–Z / 0–9)", raw: [0x80, 0x02, 0x00, 0x18] },
    MousePreset { name: "V", category: "Keys (A–Z / 0–9)", raw: [0x80, 0x02, 0x00, 0x19] },
    MousePreset { name: "W", category: "Keys (A–Z / 0–9)", raw: [0x80, 0x02, 0x00, 0x1A] },
    MousePreset { name: "X", category: "Keys (A–Z / 0–9)", raw: [0x80, 0x02, 0x00, 0x1B] },
    MousePreset { name: "Y", category: "Keys (A–Z / 0–9)", raw: [0x80, 0x02, 0x00, 0x1C] },
    MousePreset { name: "Z", category: "Keys (A–Z / 0–9)", raw: [0x80, 0x02, 0x00, 0x1D] },
    MousePreset { name: "1", category: "Keys (A–Z / 0–9)", raw: [0x80, 0x02, 0x00, 0x1E] },
    MousePreset { name: "2", category: "Keys (A–Z / 0–9)", raw: [0x80, 0x02, 0x00, 0x1F] },
    MousePreset { name: "3", category: "Keys (A–Z / 0–9)", raw: [0x80, 0x02, 0x00, 0x20] },
    MousePreset { name: "4", category: "Keys (A–Z / 0–9)", raw: [0x80, 0x02, 0x00, 0x21] },
    MousePreset { name: "5", category: "Keys (A–Z / 0–9)", raw: [0x80, 0x02, 0x00, 0x22] },
    MousePreset { name: "6", category: "Keys (A–Z / 0–9)", raw: [0x80, 0x02, 0x00, 0x23] },
    MousePreset { name: "7", category: "Keys (A–Z / 0–9)", raw: [0x80, 0x02, 0x00, 0x24] },
    MousePreset { name: "8", category: "Keys (A–Z / 0–9)", raw: [0x80, 0x02, 0x00, 0x25] },
    MousePreset { name: "9", category: "Keys (A–Z / 0–9)", raw: [0x80, 0x02, 0x00, 0x26] },
    MousePreset { name: "0", category: "Keys (A–Z / 0–9)", raw: [0x80, 0x02, 0x00, 0x27] },
    // F-Keys & Navigation
    MousePreset { name: "F1", category: "F1–F12 & Nav", raw: [0x80, 0x02, 0x00, 0x3A] },
    MousePreset { name: "F2", category: "F1–F12 & Nav", raw: [0x80, 0x02, 0x00, 0x3B] },
    MousePreset { name: "F3", category: "F1–F12 & Nav", raw: [0x80, 0x02, 0x00, 0x3C] },
    MousePreset { name: "F4", category: "F1–F12 & Nav", raw: [0x80, 0x02, 0x00, 0x3D] },
    MousePreset { name: "F5", category: "F1–F12 & Nav", raw: [0x80, 0x02, 0x00, 0x3E] },
    MousePreset { name: "F6", category: "F1–F12 & Nav", raw: [0x80, 0x02, 0x00, 0x3F] },
    MousePreset { name: "F7", category: "F1–F12 & Nav", raw: [0x80, 0x02, 0x00, 0x40] },
    MousePreset { name: "F8", category: "F1–F12 & Nav", raw: [0x80, 0x02, 0x00, 0x41] },
    MousePreset { name: "F9", category: "F1–F12 & Nav", raw: [0x80, 0x02, 0x00, 0x42] },
    MousePreset { name: "F10", category: "F1–F12 & Nav", raw: [0x80, 0x02, 0x00, 0x43] },
    MousePreset { name: "F11", category: "F1–F12 & Nav", raw: [0x80, 0x02, 0x00, 0x44] },
    MousePreset { name: "F12", category: "F1–F12 & Nav", raw: [0x80, 0x02, 0x00, 0x45] },
    MousePreset { name: "Up Arrow", category: "F1–F12 & Nav", raw: [0x80, 0x02, 0x00, 0x52] },
    MousePreset { name: "Down Arrow", category: "F1–F12 & Nav", raw: [0x80, 0x02, 0x00, 0x51] },
    MousePreset { name: "Left Arrow", category: "F1–F12 & Nav", raw: [0x80, 0x02, 0x00, 0x50] },
    MousePreset { name: "Right Arrow", category: "F1–F12 & Nav", raw: [0x80, 0x02, 0x00, 0x4F] },
    MousePreset { name: "Delete", category: "F1–F12 & Nav", raw: [0x80, 0x02, 0x00, 0x4C] },
    MousePreset { name: "Insert", category: "F1–F12 & Nav", raw: [0x80, 0x02, 0x00, 0x49] },
    MousePreset { name: "Home", category: "F1–F12 & Nav", raw: [0x80, 0x02, 0x00, 0x4A] },
    MousePreset { name: "End", category: "F1–F12 & Nav", raw: [0x80, 0x02, 0x00, 0x4D] },
    MousePreset { name: "Page Up", category: "F1–F12 & Nav", raw: [0x80, 0x02, 0x00, 0x4B] },
    MousePreset { name: "Page Down", category: "F1–F12 & Nav", raw: [0x80, 0x02, 0x00, 0x4E] },
    // DPI & Special
    MousePreset { name: "DPI Cycle", category: "DPI / Special", raw: [0x90, 0x05, 0x00, 0x00] },
    MousePreset { name: "DPI Up", category: "DPI / Special", raw: [0x90, 0x03, 0x00, 0x00] },
    MousePreset { name: "DPI Down", category: "DPI / Special", raw: [0x90, 0x04, 0x00, 0x00] },
    MousePreset { name: "Default DPI", category: "DPI / Special", raw: [0x90, 0x06, 0x00, 0x00] },
    MousePreset { name: "DPI Shift (Sniper)", category: "DPI / Special", raw: [0x90, 0x07, 0x00, 0x00] },
    // Shortcuts
    MousePreset { name: "Copy (Ctrl+C)", category: "Shortcuts", raw: [0x80, 0x02, 0x01, 0x06] },
    MousePreset { name: "Paste (Ctrl+V)", category: "Shortcuts", raw: [0x80, 0x02, 0x01, 0x19] },
    MousePreset { name: "Cut (Ctrl+X)", category: "Shortcuts", raw: [0x80, 0x02, 0x01, 0x1B] },
    MousePreset { name: "Undo (Ctrl+Z)", category: "Shortcuts", raw: [0x80, 0x02, 0x01, 0x1D] },
    MousePreset { name: "Redo (Ctrl+Y)", category: "Shortcuts", raw: [0x80, 0x02, 0x01, 0x1C] },
    MousePreset { name: "Enter", category: "Shortcuts", raw: [0x80, 0x02, 0x00, 0x28] },
    MousePreset { name: "Escape", category: "Shortcuts", raw: [0x80, 0x02, 0x00, 0x29] },
    MousePreset { name: "Space", category: "Shortcuts", raw: [0x80, 0x02, 0x00, 0x2C] },
    MousePreset { name: "Tab", category: "Shortcuts", raw: [0x80, 0x02, 0x00, 0x2B] },
    MousePreset { name: "Backspace", category: "Shortcuts", raw: [0x80, 0x02, 0x00, 0x2A] },
    // Media
    MousePreset { name: "Mute Audio", category: "Media", raw: [0x80, 0x03, 0x00, 0xE2] },
    MousePreset { name: "Volume Up", category: "Media", raw: [0x80, 0x03, 0x00, 0xE9] },
    MousePreset { name: "Volume Down", category: "Media", raw: [0x80, 0x03, 0x00, 0xEA] },
    MousePreset { name: "Play / Pause", category: "Media", raw: [0x80, 0x03, 0x00, 0xCD] },
    MousePreset { name: "Next Track", category: "Media", raw: [0x80, 0x03, 0x00, 0xB5] },
    MousePreset { name: "Previous Track", category: "Media", raw: [0x80, 0x03, 0x00, 0xB6] },
    // Disabled
    MousePreset { name: "Disabled", category: "Disabled", raw: [0xFF, 0x00, 0x00, 0x00] },
];

impl App {
    fn render_mouse_schematic(
        &mut self,
        ui: &mut egui::Ui,
        buttons: &[crate::hidpp::MouseButtonInfo],
        active_dpi_stage: Option<usize>,
    ) {
        let (response, painter) = ui.allocate_painter(Vec2::new(268.0, 385.0), Sense::click());
        let rect = response.rect;
        let cx = rect.center().x + 6.0;
        let top_y = rect.min.y + 16.0;
        let bot_y = rect.max.y - 20.0;

        let pointer_pos = response.hover_pos();
        let was_clicked = response.clicked();

        // Contoured G305 Egg Silhouette using layered rounded rects + palm dome
        let upper_shell = Rect::from_min_max(
            Pos2::new(cx - 76.0, top_y),
            Pos2::new(cx + 76.0, top_y + 185.0),
        );
        let lower_palm = Rect::from_min_max(
            Pos2::new(cx - 80.0, top_y + 110.0),
            Pos2::new(cx + 80.0, bot_y),
        );

        // Subtle ambient shadow/glow
        painter.rect_filled(
            lower_palm.expand(5.0),
            CornerRadius { nw: 50, ne: 50, sw: 76, se: 76 },
            Color32::from_rgba_premultiplied(14, 165, 233, 10),
        );

        // Base chassis
        painter.rect_filled(
            lower_palm,
            CornerRadius { nw: 42, ne: 42, sw: 74, se: 74 },
            Color32::from_rgb(16, 20, 29),
        );
        painter.rect_filled(
            upper_shell,
            CornerRadius { nw: 56, ne: 56, sw: 24, se: 24 },
            Color32::from_rgb(16, 20, 29),
        );
        painter.rect_stroke(
            lower_palm,
            CornerRadius { nw: 42, ne: 42, sw: 74, se: 74 },
            Stroke::new(1.5, Theme::BORDER_CARD),
            StrokeKind::Inside,
        );
        painter.rect_stroke(
            upper_shell,
            CornerRadius { nw: 56, ne: 56, sw: 24, se: 24 },
            Stroke::new(1.5, Theme::BORDER_CARD),
            StrokeKind::Inside,
        );

        // Palm separator seam line
        painter.line_segment(
            [Pos2::new(cx - 72.0, top_y + 132.0), Pos2::new(cx + 72.0, top_y + 132.0)],
            Stroke::new(1.2, Color32::from_rgb(10, 13, 18)),
        );

        // Helper for interactive zone check
        let mut hovered_any = false;
        let mut check_zone = |idx: u8, zone: Rect| -> (bool, bool) {
            let hov = pointer_pos.is_some_and(|p| zone.contains(p));
            if hov {
                hovered_any = true;
                if was_clicked {
                    self.selected_mouse_button = idx;
                }
            }
            let sel = self.selected_mouse_button == idx;
            (sel, hov)
        };

        // 1. Left Click Plate (Index 0)
        let left_plate = Rect::from_min_max(
            Pos2::new(cx - 71.0, top_y + 5.0),
            Pos2::new(cx - 4.0, top_y + 128.0),
        );
        let (l_sel, l_hov) = check_zone(0, left_plate);
        painter.rect_filled(
            left_plate,
            CornerRadius { nw: 52, ne: 6, sw: 8, se: 6 },
            if l_sel {
                Theme::BG_SELECTED
            } else if l_hov {
                Theme::BG_HOVER
            } else {
                Color32::from_rgb(23, 29, 41)
            },
        );
        painter.rect_stroke(
            left_plate,
            CornerRadius { nw: 52, ne: 6, sw: 8, se: 6 },
            Stroke::new(
                if l_sel { 1.5 } else { 1.0 },
                if l_sel {
                    Theme::ACCENT_BRIGHT
                } else if l_hov {
                    Theme::BORDER_HOVER
                } else {
                    Theme::BORDER_CARD
                },
            ),
            StrokeKind::Inside,
        );

        // 2. Right Click Plate (Index 1)
        let right_plate = Rect::from_min_max(
            Pos2::new(cx + 4.0, top_y + 5.0),
            Pos2::new(cx + 71.0, top_y + 128.0),
        );
        let (r_sel, r_hov) = check_zone(1, right_plate);
        painter.rect_filled(
            right_plate,
            CornerRadius { nw: 6, ne: 52, sw: 6, se: 8 },
            if r_sel {
                Theme::BG_SELECTED
            } else if r_hov {
                Theme::BG_HOVER
            } else {
                Color32::from_rgb(23, 29, 41)
            },
        );
        painter.rect_stroke(
            right_plate,
            CornerRadius { nw: 6, ne: 52, sw: 6, se: 8 },
            Stroke::new(
                if r_sel { 1.5 } else { 1.0 },
                if r_sel {
                    Theme::ACCENT_BRIGHT
                } else if r_hov {
                    Theme::BORDER_HOVER
                } else {
                    Theme::BORDER_CARD
                },
            ),
            StrokeKind::Inside,
        );

        // 3. Center Island & Scroll Wheel (Index 2)
        let island = Rect::from_min_max(
            Pos2::new(cx - 13.0, top_y + 18.0),
            Pos2::new(cx + 13.0, top_y + 124.0),
        );
        painter.rect_filled(island, CornerRadius::same(12), Color32::from_rgb(12, 15, 22));

        let wheel = Rect::from_min_max(
            Pos2::new(cx - 9.0, top_y + 24.0),
            Pos2::new(cx + 9.0, top_y + 82.0),
        );
        let (w_sel, w_hov) = check_zone(2, wheel.expand(3.0));
        painter.rect_filled(
            wheel,
            CornerRadius::same(7),
            if w_sel {
                Theme::SUCCESS_DIM
            } else if w_hov {
                Theme::BG_HOVER
            } else {
                Color32::from_rgb(28, 35, 48)
            },
        );
        painter.rect_stroke(
            wheel,
            CornerRadius::same(7),
            Stroke::new(
                if w_sel { 1.5 } else { 1.0 },
                if w_sel { Theme::SUCCESS_BRIGHT } else { Theme::BORDER_HOVER },
            ),
            StrokeKind::Inside,
        );
        for ry in (31..77).step_by(6) {
            painter.line_segment(
                [Pos2::new(cx - 5.5, top_y + ry as f32), Pos2::new(cx + 5.5, top_y + ry as f32)],
                Stroke::new(1.0, Color32::from_rgb(52, 65, 88)),
            );
        }

        // 4. DPI Cycle Button (Index 5) + Stage Status LED
        let dpi_btn = Rect::from_center_size(Pos2::new(cx, top_y + 102.0), Vec2::new(22.0, 22.0));
        let (d_sel, d_hov) = check_zone(5, dpi_btn.expand(4.0));
        painter.rect_filled(
            dpi_btn,
            CornerRadius::same(6),
            if d_sel {
                Theme::WARNING_DIM
            } else if d_hov {
                Theme::BG_HOVER
            } else {
                Color32::from_rgb(26, 33, 46)
            },
        );
        painter.rect_stroke(
            dpi_btn,
            CornerRadius::same(6),
            Stroke::new(
                if d_sel { 1.5 } else { 1.0 },
                if d_sel { Theme::WARNING } else { Theme::BORDER_HOVER },
            ),
            StrokeKind::Inside,
        );

        // Active DPI LED diamond
        let stage_colors = [
            Color32::from_rgb(234, 179, 8),
            Color32::from_rgb(249, 115, 22),
            Color32::from_rgb(14, 165, 233),
            Color32::from_rgb(168, 85, 247),
            Color32::from_rgb(16, 185, 129),
        ];
        let led_col = active_dpi_stage
            .and_then(|s| stage_colors.get(s).copied())
            .unwrap_or(Theme::ACCENT_BRIGHT);
        painter.circle_filled(Pos2::new(cx, top_y + 121.0), 2.8, led_col);

        // 5. Left-Side Thumb Buttons: Forward (Index 4) & Back (Index 3)
        let fwd_btn = Rect::from_min_max(
            Pos2::new(cx - 88.0, top_y + 142.0),
            Pos2::new(cx - 68.0, top_y + 184.0),
        );
        let (f_sel, f_hov) = check_zone(4, fwd_btn.expand(3.0));
        painter.rect_filled(
            fwd_btn,
            CornerRadius { nw: 8, ne: 4, sw: 4, se: 4 },
            if f_sel {
                Theme::PURPLE_DIM
            } else if f_hov {
                Theme::BG_HOVER
            } else {
                Color32::from_rgb(25, 31, 44)
            },
        );
        painter.rect_stroke(
            fwd_btn,
            CornerRadius { nw: 8, ne: 4, sw: 4, se: 4 },
            Stroke::new(if f_sel { 1.5 } else { 1.0 }, if f_sel { Theme::PURPLE } else { Theme::BORDER_HOVER }),
            StrokeKind::Inside,
        );

        let back_btn = Rect::from_min_max(
            Pos2::new(cx - 88.0, top_y + 190.0),
            Pos2::new(cx - 68.0, top_y + 232.0),
        );
        let (b_sel, b_hov) = check_zone(3, back_btn.expand(3.0));
        painter.rect_filled(
            back_btn,
            CornerRadius { nw: 4, ne: 4, sw: 8, se: 4 },
            if b_sel {
                Theme::PURPLE_DIM
            } else if b_hov {
                Theme::BG_HOVER
            } else {
                Color32::from_rgb(25, 31, 44)
            },
        );
        painter.rect_stroke(
            back_btn,
            CornerRadius { nw: 4, ne: 4, sw: 8, se: 4 },
            Stroke::new(if b_sel { 1.5 } else { 1.0 }, if b_sel { Theme::PURPLE } else { Theme::BORDER_HOVER }),
            StrokeKind::Inside,
        );

        if hovered_any {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }

        // Numbered Callout Badges on the Mouse
        let badges: [(u8, Pos2, Color32); 6] = [
            (0, Pos2::new(left_plate.center().x - 4.0, left_plate.center().y - 6.0), Theme::ACCENT),
            (1, Pos2::new(right_plate.center().x + 4.0, right_plate.center().y - 6.0), Theme::ACCENT),
            (2, Pos2::new(wheel.center().x, wheel.center().y), Theme::SUCCESS),
            (5, Pos2::new(dpi_btn.center().x, dpi_btn.center().y), Theme::WARNING),
            (4, Pos2::new(fwd_btn.center().x, fwd_btn.center().y), Theme::PURPLE),
            (3, Pos2::new(back_btn.center().x, back_btn.center().y), Theme::PURPLE),
        ];

        for (idx, pos, col) in badges {
            let is_sel = self.selected_mouse_button == idx;
            let r = if is_sel { 10.5 } else { 8.5 };
            if is_sel {
                painter.circle_filled(pos, r + 3.5, col.gamma_multiply(0.28));
            }
            painter.circle_filled(pos, r, if is_sel { col } else { Theme::BG_ELEVATED });
            painter.circle_stroke(pos, r, Stroke::new(1.2, col));
            painter.text(
                pos,
                egui::Align2::CENTER_CENTER,
                format!("{}", idx + 1),
                FontId::proportional(10.5),
                if is_sel { Color32::from_rgb(8, 10, 14) } else { Theme::TEXT_PRIMARY },
            );
        }

        // Labels on L / R paddles
        if let Some(b0) = buttons.first() {
            painter.text(
                Pos2::new(left_plate.center().x - 4.0, left_plate.center().y + 18.0),
                egui::Align2::CENTER_CENTER,
                &b0.binding_label,
                FontId::proportional(9.5),
                Theme::TEXT_SECONDARY,
            );
        }
        if let Some(b1) = buttons.get(1) {
            painter.text(
                Pos2::new(right_plate.center().x + 4.0, right_plate.center().y + 18.0),
                egui::Align2::CENTER_CENTER,
                &b1.binding_label,
                FontId::proportional(9.5),
                Theme::TEXT_SECONDARY,
            );
        }

        // Subtle G Emblem on Palm Rest
        let palm_center = Pos2::new(cx, top_y + 258.0);
        painter.circle_filled(palm_center, 22.0, Color32::from_rgba_premultiplied(14, 165, 233, 12));
        painter.text(
            Pos2::new(palm_center.x, palm_center.y - 1.0),
            egui::Align2::CENTER_CENTER,
            "G",
            FontId::proportional(24.0),
            Theme::ACCENT_BRIGHT.gamma_multiply(0.75),
        );
        painter.text(
            Pos2::new(cx, palm_center.y + 32.0),
            egui::Align2::CENTER_CENTER,
            "LIGHTSPEED ONBOARD",
            FontId::proportional(8.5),
            Theme::TEXT_MUTED,
        );
    }

    fn render_mouse_buttons_tab(&mut self, ui: &mut egui::Ui, snapshot: &Snapshot) {
        let Some(buttons) = &snapshot.mouse_buttons else {
            ui.label("Mouse button remapping is not supported on this device.");
            return;
        };

        let defaults: [[u8; 4]; 6] = [
            [0x80, 0x01, 0x00, 0x01],
            [0x80, 0x01, 0x00, 0x02],
            [0x80, 0x01, 0x00, 0x04],
            [0x80, 0x01, 0x00, 0x08],
            [0x80, 0x01, 0x00, 0x10],
            [0x90, 0x05, 0x00, 0x00],
        ];
        let default_names = [
            "Left Click",
            "Right Click",
            "Middle Click",
            "Back (Side 1)",
            "Forward (Side 2)",
            "DPI Cycle",
        ];

        let active_dpi = snapshot.dpi.as_ref().map(|d| d.active_index);

        ui.horizontal_top(|ui| {
            // Left Column: Interactive Hardware Blueprint
            egui::Frame::new()
                .fill(Theme::BG_CARD)
                .stroke(Stroke::new(1.0, Theme::BORDER_SUBTLE))
                .corner_radius(CornerRadius::same(12))
                .inner_margin(Margin::symmetric(14, 14))
                .show(ui, |ui| {
                    ui.set_width(272.0);
                    ui.vertical_centered(|ui| {
                        pill_badge(ui, "INTERACTIVE BLUEPRINT", Theme::ACCENT_BRIGHT, Theme::ACCENT_DIM);
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new("Click any numbered zone to select")
                                .size(11.0)
                                .color(Theme::TEXT_MUTED),
                        );
                        self.render_mouse_schematic(ui, buttons, active_dpi);
                    });
                });

            ui.add_space(12.0);

            // Right Column: 6 Hardware Button Rows + Preset Studio
            ui.vertical(|ui| {
                // Header card with Reset All
                egui::Frame::new()
                    .fill(Theme::BG_CARD)
                    .stroke(Stroke::new(1.0, Theme::BORDER_SUBTLE))
                    .corner_radius(CornerRadius::same(12))
                    .inner_margin(Margin::symmetric(16, 12))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.label(
                                    RichText::new("Onboard Hardware Button Assignments")
                                        .size(15.5)
                                        .strong()
                                        .color(Theme::TEXT_PRIMARY),
                                );
                                ui.label(
                                    RichText::new("Flashed directly to mouse EEPROM (0x8100) — zero background lag in games.")
                                        .size(11.5)
                                        .color(Theme::TEXT_MUTED),
                                );
                            });
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if toolbar_button(ui, IconKind::Reset, "Reset Defaults", false) {
                                    self.reset_all_mouse_buttons();
                                }
                            });
                        });
                    });

                ui.add_space(8.0);

                // 6 Button Assignment Cards
                for btn in buttons {
                    let idx = btn.index as usize;
                    let default_raw = defaults.get(idx).copied().unwrap_or([0, 0, 0, 0]);
                    let is_custom = btn.raw != default_raw;
                    let is_selected = self.selected_mouse_button == btn.index;

                    let accent_col = match btn.index {
                        0 | 1 => Theme::ACCENT,
                        2 => Theme::SUCCESS,
                        3 | 4 => Theme::PURPLE,
                        _ => Theme::WARNING,
                    };

                    let card_bg = if is_selected {
                        Theme::BG_SELECTED
                    } else {
                        Theme::BG_CARD
                    };
                    let card_border = if is_selected {
                        Stroke::new(1.5, Theme::ACCENT)
                    } else if is_custom {
                        Stroke::new(1.0, Theme::ACCENT.gamma_multiply(0.5))
                    } else {
                        Stroke::new(1.0, Theme::BORDER_SUBTLE)
                    };

                    let resp = egui::Frame::new()
                        .fill(card_bg)
                        .stroke(card_border)
                        .corner_radius(CornerRadius::same(10))
                        .inner_margin(Margin::symmetric(14, 9))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                // Clickable number badge
                                let (b_rect, b_resp) = ui.allocate_exact_size(Vec2::splat(26.0), Sense::click());
                                if b_resp.clicked() {
                                    self.selected_mouse_button = btn.index;
                                }
                                ui.painter().circle_filled(
                                    b_rect.center(),
                                    12.0,
                                    if is_selected { accent_col } else { Theme::BG_ELEVATED },
                                );
                                ui.painter().circle_stroke(b_rect.center(), 12.0, Stroke::new(1.2, accent_col));
                                ui.painter().text(
                                    b_rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    format!("{}", btn.index + 1),
                                    FontId::proportional(11.5),
                                    if is_selected { Color32::from_rgb(8, 10, 14) } else { Theme::TEXT_PRIMARY },
                                );

                                ui.add_space(6.0);

                                ui.vertical(|ui| {
                                    ui.horizontal(|ui| {
                                        let lbl = ui.add(
                                            egui::Label::new(
                                                RichText::new(&btn.name)
                                                    .size(13.5)
                                                    .strong()
                                                    .color(Theme::TEXT_PRIMARY),
                                            )
                                            .sense(Sense::click()),
                                        );
                                        if lbl.clicked() {
                                            self.selected_mouse_button = btn.index;
                                        }
                                        if is_custom {
                                            pill_badge(ui, "CUSTOM", Theme::ACCENT_BRIGHT, Theme::ACCENT_DIM);
                                        }
                                    });
                                    ui.label(
                                        RichText::new(format!("{} • Default: {}", btn.category, default_names.get(idx).unwrap_or(&"Default")))
                                            .size(11.0)
                                            .color(Theme::TEXT_MUTED),
                                    );
                                });

                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if is_custom && ui.small_button("Reset").clicked() {
                                        let def_name = default_names.get(idx).unwrap_or(&"Default");
                                        self.recording_mouse_button = None;
                                        self.set_mouse_button(btn.index, default_raw, def_name);
                                    }

                                    let is_recording = self.recording_mouse_button == Some(btn.index);
                                    let rec_label = if is_recording {
                                        RichText::new("Press any key...").color(Theme::WARNING).strong().size(11.5)
                                    } else {
                                        RichText::new("Bind Key").color(Theme::ACCENT_BRIGHT).size(11.5)
                                    };
                                    if ui.add(egui::Button::new(rec_label)).clicked() {
                                        self.selected_mouse_button = btn.index;
                                        if is_recording {
                                            self.recording_mouse_button = None;
                                            self.set_status("Key binding cancelled", false);
                                        } else {
                                            self.recording_mouse_button = Some(btn.index);
                                            self.set_status(
                                                &format!("Press any key or shortcut on your keyboard to bind to {}...", btn.name),
                                                false,
                                            );
                                        }
                                    }

                                    egui::ComboBox::from_id_salt(format!("mouse_btn_combo_{}", btn.index))
                                        .selected_text(RichText::new(&btn.binding_label).strong().size(12.5))
                                        .width(160.0)
                                        .show_ui(ui, |ui| {
                                            let mut current_cat = "";
                                            for preset in MOUSE_PRESETS {
                                                if preset.category != current_cat {
                                                    current_cat = preset.category;
                                                    ui.separator();
                                                    ui.label(
                                                        RichText::new(current_cat)
                                                            .size(10.0)
                                                            .color(Theme::ACCENT_BRIGHT)
                                                            .strong(),
                                                    );
                                                }
                                                let is_preset_sel = btn.raw == preset.raw;
                                                if ui.selectable_label(is_preset_sel, preset.name).clicked() {
                                                    self.selected_mouse_button = btn.index;
                                                    self.recording_mouse_button = None;
                                                    self.set_mouse_button(btn.index, preset.raw, preset.name);
                                                }
                                            }
                                        });
                                });
                            });
                        })
                        .response;

                    if resp.clicked() {
                        self.selected_mouse_button = btn.index;
                    }
                    ui.add_space(4.0);
                }

                ui.add_space(4.0);

                // Quick-Assign Preset Palette for Selected Button
                let sel_idx = (self.selected_mouse_button as usize).min(buttons.len().saturating_sub(1));
                if let Some(sel_btn) = buttons.get(sel_idx) {
                    egui::Frame::new()
                        .fill(Theme::BG_CARD)
                        .stroke(Stroke::new(1.0, Theme::BORDER_SUBTLE))
                        .corner_radius(CornerRadius::same(10))
                        .inner_margin(Margin::symmetric(14, 10))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(format!("Quick Assign to Button {} ({}):", sel_btn.index + 1, sel_btn.name))
                                        .size(12.0)
                                        .strong()
                                        .color(Theme::ACCENT_BRIGHT),
                                );
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    for (filter, title) in [
                                        (MousePresetFilter::Media, "Media"),
                                        (MousePresetFilter::Shortcuts, "Shortcuts"),
                                        (MousePresetFilter::DpiSpecial, "DPI"),
                                        (MousePresetFilter::Mouse, "Mouse"),
                                        (MousePresetFilter::FunctionNav, "F1–F12 & Nav"),
                                        (MousePresetFilter::Keys, "Keys (A–Z / 0–9)"),
                                        (MousePresetFilter::All, "All"),
                                    ] {
                                        let active = self.mouse_preset_filter == filter;
                                        if ui
                                            .selectable_label(
                                                active,
                                                RichText::new(title).size(11.0).strong(),
                                            )
                                            .clicked()
                                        {
                                            self.mouse_preset_filter = filter;
                                        }
                                    }
                                });
                            });
                            ui.add_space(6.0);
                            ui.horizontal_wrapped(|ui| {
                                for preset in MOUSE_PRESETS {
                                    let show = match self.mouse_preset_filter {
                                        MousePresetFilter::All => true,
                                        MousePresetFilter::Keys => preset.category == "Keys (A–Z / 0–9)",
                                        MousePresetFilter::FunctionNav => preset.category == "F1–F12 & Nav",
                                        MousePresetFilter::Mouse => preset.category == "Mouse",
                                        MousePresetFilter::DpiSpecial => preset.category == "DPI / Special",
                                        MousePresetFilter::Shortcuts => preset.category == "Shortcuts",
                                        MousePresetFilter::Media => preset.category == "Media",
                                    };
                                    if !show {
                                        continue;
                                    }
                                    let is_curr = sel_btn.raw == preset.raw;
                                    if ui
                                        .selectable_label(
                                            is_curr,
                                            RichText::new(preset.name).size(11.5),
                                        )
                                        .clicked()
                                    {
                                        self.recording_mouse_button = None;
                                        self.set_mouse_button(sel_btn.index, preset.raw, preset.name);
                                    }
                                }
                            });
                        });
                }
            });
        });
    }

    // ========================================================================
    // Tab 2: Unified Sensor DPI, Polling Rate & Power Mode
    // ========================================================================

    fn render_sensor_and_power_tab(&mut self, ui: &mut egui::Ui, snapshot: &Snapshot) {
        egui::ScrollArea::vertical()
            .id_salt("sensor_power_scroll")
            .show(ui, |ui| {
                // 1. HERO Optical Sensor DPI Stages
                if let Some(dpi) = &snapshot.dpi {
                    egui::Frame::new()
                        .fill(Theme::BG_CARD)
                        .stroke(Stroke::new(1.0, Theme::BORDER_SUBTLE))
                        .corner_radius(CornerRadius::same(12))
                        .inner_margin(Margin::same(18))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.label(
                                        RichText::new("HERO Optical Sensor DPI Stages")
                                            .size(16.0)
                                            .strong()
                                            .color(Theme::TEXT_PRIMARY),
                                    );
                                    ui.label(
                                        RichText::new("Click any hardware DPI profile card to switch active tracking sensitivity immediately.")
                                            .size(12.0)
                                            .color(Theme::TEXT_MUTED),
                                    );
                                });
                            });

                            ui.add_space(14.0);

                            let stage_colors = [
                                (Color32::from_rgb(234, 179, 8), "Yellow LED"),
                                (Color32::from_rgb(249, 115, 22), "Orange LED"),
                                (Color32::from_rgb(14, 165, 233), "Cyan LED"),
                                (Color32::from_rgb(168, 85, 247), "Purple LED"),
                                (Color32::from_rgb(16, 185, 129), "Green LED"),
                            ];

                            let count = dpi.levels.len().max(1);
                            let avail_w = ui.available_width();
                            let card_w = ((avail_w - (count.saturating_sub(1) as f32 * 10.0)) / count as f32).clamp(130.0, 210.0);

                            let mut clicked_stage = None;
                            ui.horizontal(|ui| {
                                for (idx, &level) in dpi.levels.iter().enumerate() {
                                    let is_active = idx == dpi.active_index;
                                    let (accent, led_name) = stage_colors
                                        .get(idx)
                                        .copied()
                                        .unwrap_or((Theme::ACCENT, "Stage LED"));

                                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(card_w, 116.0), Sense::click());
                                    if resp.hovered() {
                                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                    }
                                    if resp.clicked() && !is_active {
                                        clicked_stage = Some(idx);
                                    }

                                    if ui.is_rect_visible(rect) {
                                        let painter = ui.painter();
                                        let bg = if is_active {
                                            Theme::BG_SELECTED
                                        } else if resp.hovered() {
                                            Theme::BG_HOVER
                                        } else {
                                            Theme::BG_CARD_ALT
                                        };
                                        let border = if is_active {
                                            Stroke::new(1.5, accent)
                                        } else if resp.hovered() {
                                            Stroke::new(1.0, Theme::BORDER_HOVER)
                                        } else {
                                            Stroke::new(1.0, Theme::BORDER_SUBTLE)
                                        };

                                        painter.rect_filled(rect, CornerRadius::same(10), bg);
                                        painter.rect_stroke(rect, CornerRadius::same(10), border, StrokeKind::Inside);

                                        // Stage header + LED dot
                                        painter.circle_filled(Pos2::new(rect.min.x + 18.0, rect.min.y + 20.0), 4.5, accent);
                                        painter.text(
                                            Pos2::new(rect.min.x + 28.0, rect.min.y + 20.0),
                                            egui::Align2::LEFT_CENTER,
                                            format!("STAGE {}", idx + 1),
                                            FontId::proportional(10.5),
                                            accent,
                                        );

                                        if is_active {
                                            painter.text(
                                                Pos2::new(rect.max.x - 12.0, rect.min.y + 20.0),
                                                egui::Align2::RIGHT_CENTER,
                                                "ACTIVE",
                                                FontId::proportional(9.5),
                                                Theme::SUCCESS_BRIGHT,
                                            );
                                        }

                                        // Large DPI Readout
                                        painter.text(
                                            Pos2::new(rect.min.x + 14.0, rect.min.y + 54.0),
                                            egui::Align2::LEFT_CENTER,
                                            format!("{level}"),
                                            FontId::proportional(26.0),
                                            Theme::TEXT_PRIMARY,
                                        );
                                        painter.text(
                                            Pos2::new(rect.min.x + 14.0, rect.min.y + 78.0),
                                            egui::Align2::LEFT_CENTER,
                                            led_name,
                                            FontId::proportional(11.0),
                                            Theme::TEXT_MUTED,
                                        );

                                        // Relative sensitivity bar at bottom of card
                                        let bar_bg = Rect::from_min_max(
                                            Pos2::new(rect.min.x + 14.0, rect.max.y - 16.0),
                                            Pos2::new(rect.max.x - 14.0, rect.max.y - 11.0),
                                        );
                                        painter.rect_filled(bar_bg, CornerRadius::same(2), Theme::BG_ELEVATED);
                                        let frac = (level as f32 / 3200.0).clamp(0.12, 1.0);
                                        let bar_fill = Rect::from_min_max(
                                            bar_bg.min,
                                            Pos2::new(bar_bg.min.x + bar_bg.width() * frac, bar_bg.max.y),
                                        );
                                        painter.rect_filled(bar_fill, CornerRadius::same(2), accent);
                                    }
                                }
                            });

                            if let Some(stage) = clicked_stage {
                                self.set_dpi_stage(stage);
                            }
                        });

                    ui.add_space(14.0);
                }

                // 2. USB / Lightspeed Polling Rate
                if snapshot.polling_rate_hz.is_some() {
                    egui::Frame::new()
                        .fill(Theme::BG_CARD)
                        .stroke(Stroke::new(1.0, Theme::BORDER_SUBTLE))
                        .corner_radius(CornerRadius::same(12))
                        .inner_margin(Margin::same(18))
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new("Report Rate (Polling Frequency)")
                                    .size(16.0)
                                    .strong()
                                    .color(Theme::TEXT_PRIMARY),
                            );
                            ui.label(
                                RichText::new("Higher report rates reduce cursor input latency; lower rates conserve wireless battery.")
                                    .size(12.0)
                                    .color(Theme::TEXT_MUTED),
                            );
                            ui.add_space(14.0);

                            let current_hz = snapshot.polling_rate_hz.unwrap_or(1000);
                            let rate_options = [
                                (1000u16, "1 ms Delay", "Esports Competitive", Theme::ACCENT_BRIGHT),
                                (500u16, "2 ms Delay", "High Precision Gaming", Theme::ACCENT),
                                (250u16, "4 ms Delay", "Balanced Daily Use", Theme::TEXT_SECONDARY),
                                (125u16, "8 ms Delay", "Max Battery Endurance", Theme::SUCCESS_BRIGHT),
                            ];

                            let avail_w = ui.available_width();
                            let card_w = ((avail_w - 30.0) / 4.0).clamp(135.0, 210.0);
                            let mut clicked_rate = None;

                            ui.horizontal(|ui| {
                                for (rate, delay, desc, col) in rate_options {
                                    let is_active = rate == current_hz;
                                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(card_w, 92.0), Sense::click());
                                    if resp.hovered() {
                                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                    }
                                    if resp.clicked() && !is_active {
                                        clicked_rate = Some(rate);
                                    }

                                    if ui.is_rect_visible(rect) {
                                        let painter = ui.painter();
                                        let bg = if is_active {
                                            Theme::BG_SELECTED
                                        } else if resp.hovered() {
                                            Theme::BG_HOVER
                                        } else {
                                            Theme::BG_CARD_ALT
                                        };
                                        let border = if is_active {
                                            Stroke::new(1.5, Theme::ACCENT)
                                        } else if resp.hovered() {
                                            Stroke::new(1.0, Theme::BORDER_HOVER)
                                        } else {
                                            Stroke::new(1.0, Theme::BORDER_SUBTLE)
                                        };
                                        painter.rect_filled(rect, CornerRadius::same(10), bg);
                                        painter.rect_stroke(rect, CornerRadius::same(10), border, StrokeKind::Inside);

                                        painter.text(
                                            Pos2::new(rect.min.x + 14.0, rect.min.y + 24.0),
                                            egui::Align2::LEFT_CENTER,
                                            format!("{rate} Hz"),
                                            FontId::proportional(19.0),
                                            Theme::TEXT_PRIMARY,
                                        );
                                        if is_active {
                                            painter.circle_filled(Pos2::new(rect.max.x - 16.0, rect.min.y + 24.0), 4.5, Theme::ACCENT_BRIGHT);
                                        }
                                        painter.text(
                                            Pos2::new(rect.min.x + 14.0, rect.min.y + 50.0),
                                            egui::Align2::LEFT_CENTER,
                                            delay,
                                            FontId::proportional(11.5),
                                            col,
                                        );
                                        painter.text(
                                            Pos2::new(rect.min.x + 14.0, rect.min.y + 70.0),
                                            egui::Align2::LEFT_CENTER,
                                            desc,
                                            FontId::proportional(11.0),
                                            Theme::TEXT_MUTED,
                                        );
                                    }
                                }
                            });

                            if let Some(rate) = clicked_rate {
                                self.set_polling_rate(rate);
                            }
                        });

                    ui.add_space(14.0);
                }

                // 3. Hardware Power Mode (LO/HI Performance vs Endurance)
                if let Some(mode) = snapshot.mode_status {
                    egui::Frame::new()
                        .fill(Theme::BG_CARD)
                        .stroke(Stroke::new(1.0, Theme::BORDER_SUBTLE))
                        .corner_radius(CornerRadius::same(12))
                        .inner_margin(Margin::same(18))
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new("Onboard Power Operating Profile")
                                    .size(16.0)
                                    .strong()
                                    .color(Theme::TEXT_PRIMARY),
                            );
                            ui.label(
                                RichText::new("Switch hardware microcontroller between esports 1ms responsiveness and multi-month battery endurance.")
                                    .size(12.0)
                                    .color(Theme::TEXT_MUTED),
                            );
                            ui.add_space(14.0);

                            let avail_w = ui.available_width();
                            let card_w = ((avail_w - 12.0) * 0.5).clamp(220.0, 420.0);
                            let mut clicked_mode = None;

                            ui.horizontal(|ui| {
                                let modes = [
                                    (
                                        1u8,
                                        "HI — Performance Mode",
                                        "1000 Hz (1 ms) • Cyan Indicator LED",
                                        "Maximum sensor responsiveness for competitive gaming.",
                                        Theme::ACCENT_BRIGHT,
                                    ),
                                    (
                                        2u8,
                                        "LO — Endurance Mode",
                                        "125 Hz (8 ms) • Green Indicator LED",
                                        "Eco power profile extending single AA battery up to 9 months.",
                                        Theme::SUCCESS_BRIGHT,
                                    ),
                                ];

                                for (m_id, title, subtitle, desc, col) in modes {
                                    let is_active = mode == m_id;
                                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(card_w, 98.0), Sense::click());
                                    if resp.hovered() {
                                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                    }
                                    if resp.clicked() && !is_active {
                                        clicked_mode = Some(m_id);
                                    }

                                    if ui.is_rect_visible(rect) {
                                        let painter = ui.painter();
                                        let bg = if is_active {
                                            Theme::BG_SELECTED
                                        } else if resp.hovered() {
                                            Theme::BG_HOVER
                                        } else {
                                            Theme::BG_CARD_ALT
                                        };
                                        let border = if is_active {
                                            Stroke::new(1.5, col)
                                        } else if resp.hovered() {
                                            Stroke::new(1.0, Theme::BORDER_HOVER)
                                        } else {
                                            Stroke::new(1.0, Theme::BORDER_SUBTLE)
                                        };
                                        painter.rect_filled(rect, CornerRadius::same(10), bg);
                                        painter.rect_stroke(rect, CornerRadius::same(10), border, StrokeKind::Inside);

                                        painter.circle_filled(Pos2::new(rect.min.x + 18.0, rect.min.y + 24.0), 5.0, col);
                                        painter.text(
                                            Pos2::new(rect.min.x + 30.0, rect.min.y + 24.0),
                                            egui::Align2::LEFT_CENTER,
                                            title,
                                            FontId::proportional(15.0),
                                            Theme::TEXT_PRIMARY,
                                        );
                                        painter.text(
                                            Pos2::new(rect.min.x + 16.0, rect.min.y + 50.0),
                                            egui::Align2::LEFT_CENTER,
                                            subtitle,
                                            FontId::proportional(12.0),
                                            col,
                                        );
                                        painter.text(
                                            Pos2::new(rect.min.x + 16.0, rect.min.y + 72.0),
                                            egui::Align2::LEFT_CENTER,
                                            desc,
                                            FontId::proportional(11.5),
                                            Theme::TEXT_MUTED,
                                        );
                                    }
                                }
                            });

                            if let Some(m) = clicked_mode {
                                self.set_power_mode(m);
                            }
                        });
                }
            });
    }
}

// ============================================================================
// Tab 3: Keyboard Reprogrammable Controls (Inline Remapping Studio)
// ============================================================================

impl App {
    fn render_controls_tab(&mut self, ui: &mut egui::Ui, snapshot: &Snapshot) {
        if snapshot.controls.is_empty() {
            ui.label("This device does not expose Reprogrammable Controls V4 (0x1B04).");
            return;
        }

        let remapped_count = snapshot
            .controls
            .iter()
            .filter(|c| c.mapped_to != c.cid)
            .count();

        // Top Filter & Search Bar Card
        egui::Frame::new()
            .fill(Theme::BG_CARD)
            .stroke(Stroke::new(1.0, Theme::BORDER_SUBTLE))
            .corner_radius(CornerRadius::same(12))
            .inner_margin(Margin::symmetric(16, 12))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.search_query)
                            .hint_text("Search keys & actions (e.g. Calculator, Volume, Emoji, 0x00C7)...")
                            .desired_width(320.0),
                    );
                    if !self.search_query.is_empty() && ui.small_button("Clear").clicked() {
                        self.search_query.clear();
                    }

                    ui.add_space(8.0);
                    let only_remapped = self.filter_remapped_only;
                    if ui
                        .selectable_label(
                            only_remapped,
                            RichText::new(format!("Customized Only ({remapped_count})")).size(12.0).strong(),
                        )
                        .clicked()
                    {
                        self.filter_remapped_only = !only_remapped;
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if remapped_count > 0 && toolbar_button(ui, IconKind::Reset, "Reset All Keys", false) {
                            self.reset_all_mappings();
                        }
                        pill_badge(
                            ui,
                            &format!("{} CONTROLS", snapshot.controls.len()),
                            Theme::TEXT_SECONDARY,
                            Theme::BG_ELEVATED,
                        );
                    });
                });
            });

        ui.add_space(10.0);

        // Inline Remappable Key Cards
        let query = self.search_query.trim().to_lowercase();
        let mut pending_remap: Option<(u16, u16)> = None;

        egui::ScrollArea::vertical()
            .id_salt("controls_remap_scroll")
            .show(ui, |ui| {
                for control in &snapshot.controls {
                    let is_remapped = control.mapped_to != control.cid;
                    let is_reprog = control.flags & 0x10 != 0;

                    if self.filter_remapped_only && !is_remapped {
                        continue;
                    }
                    if !query.is_empty() {
                        let hex_cid = format!("{:04x}", control.cid);
                        if !control.name.to_lowercase().contains(&query)
                            && !control.mapped_to_name.to_lowercase().contains(&query)
                            && !hex_cid.contains(&query)
                        {
                            continue;
                        }
                    }

                    let card_bg = if is_remapped {
                        Theme::BG_SELECTED
                    } else {
                        Theme::BG_CARD
                    };
                    let card_border = if is_remapped {
                        Stroke::new(1.5, Theme::ACCENT)
                    } else {
                        Stroke::new(1.0, Theme::BORDER_SUBTLE)
                    };

                    egui::Frame::new()
                        .fill(card_bg)
                        .stroke(card_border)
                        .corner_radius(CornerRadius::same(9))
                        .inner_margin(Margin::symmetric(14, 9))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                // Keycap badge
                                egui::Frame::new()
                                    .fill(Theme::BG_ELEVATED)
                                    .stroke(Stroke::new(1.0, Theme::BORDER_CARD))
                                    .corner_radius(CornerRadius::same(6))
                                    .inner_margin(Margin::symmetric(8, 4))
                                    .show(ui, |ui| {
                                        ui.label(
                                            RichText::new(format!("0x{:04X}", control.cid))
                                                .size(10.5)
                                                .color(Theme::ACCENT_BRIGHT)
                                                .strong(),
                                        );
                                    });

                                ui.add_space(6.0);

                                ui.vertical(|ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            RichText::new(&control.name)
                                                .size(13.5)
                                                .strong()
                                                .color(Theme::TEXT_PRIMARY),
                                        );
                                        if is_remapped {
                                            pill_badge(ui, "REMAPPED", Theme::SUCCESS_BRIGHT, Theme::SUCCESS_DIM);
                                        }
                                    });
                                    ui.label(
                                        RichText::new(&control.flags_label)
                                            .size(10.5)
                                            .color(Theme::TEXT_MUTED),
                                    );
                                });

                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if is_remapped && ui.small_button("Reset").clicked() {
                                        pending_remap = Some((control.cid, control.cid));
                                    }

                                    if is_reprog {
                                        let selected_label = if is_remapped {
                                            format!("→ {}", control.mapped_to_name)
                                        } else {
                                            format!("{} (Default)", control.mapped_to_name)
                                        };
                                        egui::ComboBox::from_id_salt(format!("ctrl_remap_{}", control.cid))
                                            .selected_text(
                                                RichText::new(selected_label)
                                                    .size(12.0)
                                                    .strong()
                                                    .color(if is_remapped {
                                                        Theme::SUCCESS_BRIGHT
                                                    } else {
                                                        Theme::TEXT_PRIMARY
                                                    }),
                                            )
                                            .width(220.0)
                                            .show_ui(ui, |ui| {
                                                for target in &snapshot.controls {
                                                    let is_sel = control.mapped_to == target.cid;
                                                    let label = if target.cid == control.cid {
                                                        format!("{} (Native Default)", target.name)
                                                    } else {
                                                        format!("{} (0x{:04X})", target.name, target.cid)
                                                    };
                                                    if ui.selectable_label(is_sel, label).clicked() {
                                                        pending_remap = Some((control.cid, target.cid));
                                                    }
                                                }
                                            });
                                    } else {
                                        pill_badge(ui, "System Fixed", Theme::TEXT_MUTED, Theme::BG_ELEVATED);
                                    }
                                });
                            });
                        });

                    ui.add_space(4.0);
                }
            });

        if let Some((from, to)) = pending_remap {
            self.apply_mapping(from, to);
        }
    }

    // ========================================================================
    // Tab 4: Unified Keyboard Illumination Studio (Live Preview + Effects + Settings)
    // ========================================================================

    fn render_keyboard_schematic(&self, ui: &mut egui::Ui, snapshot: &Snapshot) {
        let Some(backlight) = &snapshot.backlight else { return };
        let (response, painter) = ui.allocate_painter(Vec2::new(360.0, 178.0), Sense::hover());
        let rect = response.rect;

        let kb_rect = Rect::from_min_size(
            Pos2::new(rect.min.x + 6.0, rect.min.y + 6.0),
            Vec2::new(348.0, 164.0),
        );

        if backlight.enabled {
            painter.rect_filled(
                kb_rect.expand(4.0),
                CornerRadius::same(12),
                Color32::from_rgba_premultiplied(14, 165, 233, 16),
            );
        }

        // Brushed dark aluminum top case
        painter.rect_filled(kb_rect, CornerRadius::same(10), Color32::from_rgb(20, 25, 36));
        painter.rect_stroke(
            kb_rect,
            CornerRadius::same(10),
            Stroke::new(1.5, Theme::BORDER_CARD),
            StrokeKind::Inside,
        );

        // Top status bar on keyboard bezel
        let bezel = Rect::from_min_size(
            Pos2::new(kb_rect.min.x + 8.0, kb_rect.min.y + 6.0),
            Vec2::new(kb_rect.width() - 16.0, 14.0),
        );
        painter.text(
            Pos2::new(bezel.min.x + 4.0, bezel.center().y),
            egui::Align2::LEFT_CENTER,
            "LOGITECH MX MECHANICAL",
            FontId::proportional(8.5),
            Theme::TEXT_MUTED,
        );
        // Caps/Battery indicator LED on top right of keyboard
        let led_col = if backlight.enabled { Theme::SUCCESS_BRIGHT } else { Theme::TEXT_MUTED };
        painter.circle_filled(Pos2::new(bezel.max.x - 6.0, bezel.center().y), 2.5, led_col);

        let time = ui.ctx().input(|i| i.time);
        if backlight.enabled && backlight.current_effect >= 2 {
            ui.ctx().request_repaint_after(Duration::from_millis(36));
        }

        let base_alpha = if backlight.enabled {
            if backlight.manual_mode {
                (backlight.level as f32 / 7.0 * 185.0 + 45.0) as u8
            } else {
                145
            }
        } else {
            0
        };

        let calc_glow = |is_mod: bool, norm_x: f32, seed: u32| -> u8 {
            if !backlight.enabled || base_alpha == 0 {
                return 0;
            }
            match backlight.current_effect {
                0 => base_alpha,
                1 => {
                    if is_mod {
                        (base_alpha as f32 * 1.35).min(255.0) as u8
                    } else {
                        (base_alpha as f32 * 0.38) as u8
                    }
                }
                2 => {
                    let phase = ((time * std::f64::consts::TAU / 2.8).sin() * 0.5 + 0.5) as f32;
                    (base_alpha as f32 * (0.15 + 0.85 * phase)).clamp(0.0, 255.0) as u8
                }
                3 => {
                    let phase = ((time * 2.8 - (norm_x as f64 * 6.28)).sin() * 0.5 + 0.5) as f32;
                    (base_alpha as f32 * (0.15 + 0.85 * phase)).clamp(0.0, 255.0) as u8
                }
                4 => {
                    let cycle = (time * 1.6 + (seed as f64 * 0.618)) % 2.4;
                    let pulse = if cycle < 0.6 { (1.0 - (cycle / 0.6)) as f32 } else { 0.15 };
                    (base_alpha as f32 * pulse.clamp(0.15, 1.0)) as u8
                }
                5 => {
                    let twinkle = ((time * 3.2 + (seed as f64 * 1.732)).sin() * 0.5 + 0.5) as f32;
                    let sharp = twinkle.powi(3);
                    (base_alpha as f32 * (0.2 + 0.8 * sharp)).clamp(0.0, 255.0) as u8
                }
                _ => base_alpha,
            }
        };

        let row_y = [
            kb_rect.min.y + 24.0,
            kb_rect.min.y + 46.0,
            kb_rect.min.y + 68.0,
            kb_rect.min.y + 90.0,
            kb_rect.min.y + 112.0,
            kb_rect.min.y + 134.0,
        ];

        for (r_idx, &ry) in row_y.iter().take(5).enumerate() {
            let cols = match r_idx {
                0..=2 => 14,
                3 => 13,
                _ => 12,
            };
            let step = (kb_rect.width() - 16.0) / cols as f32;
            for i in 0..cols {
                let kx = kb_rect.min.x + 8.0 + (i as f32 * step);
                let kw = (step - 3.0).max(14.0);
                let krect = Rect::from_min_size(Pos2::new(kx, ry), Vec2::new(kw, 17.5));
                let is_mod = r_idx == 0 || i == 0 || i == cols - 1;
                let norm_x = (kx - kb_rect.min.x) / kb_rect.width();
                let glow = calc_glow(is_mod, norm_x, (r_idx * 16 + i) as u32);
                draw_keycap(&painter, krect, is_mod, glow);
            }
        }

        // Bottom Row (Spacebar + Modifiers + Highlighted Fn / FX keys)
        let y5 = row_y[5];
        let x0 = kb_rect.min.x + 8.0;
        let bottom_keys = [
            (0.0, 26.0, true, None),
            (29.0, 24.0, true, None),
            (56.0, 24.0, true, None),
            (83.0, 118.0, false, None),
            (204.0, 24.0, true, None),
            (231.0, 26.0, true, Some(("Fn", Theme::ACCENT_BRIGHT))),
            (260.0, 26.0, true, Some(("FX", Theme::WARNING))),
            (289.0, 42.0, true, None),
        ];

        for (idx, (dx, w, is_mod, accent)) in bottom_keys.iter().enumerate() {
            let krect = Rect::from_min_size(Pos2::new(x0 + dx, y5), Vec2::new(*w, 18.5));
            if let Some((lbl, col)) = accent {
                draw_keycap_accent(&painter, krect, lbl, *col);
            } else {
                let norm_x = (krect.center().x - kb_rect.min.x) / kb_rect.width();
                let glow = calc_glow(*is_mod, norm_x, 90 + idx as u32);
                draw_keycap(&painter, krect, *is_mod, glow);
            }
        }
    }

    fn render_backlight_tab(&mut self, ui: &mut egui::Ui, snapshot: &Snapshot) {
        let Some(backlight) = &snapshot.backlight else {
            ui.label("Backlight feature is not supported by this device.");
            return;
        };

        egui::ScrollArea::vertical()
            .id_salt("backlight_studio_scroll")
            .show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    // Left Column: Live Keyboard Simulator + 6 Effect Presets
                    egui::Frame::new()
                        .fill(Theme::BG_CARD)
                        .stroke(Stroke::new(1.0, Theme::BORDER_SUBTLE))
                        .corner_radius(CornerRadius::same(12))
                        .inner_margin(Margin::same(16))
                        .show(ui, |ui| {
                            ui.set_width(368.0);
                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    pill_badge(ui, "LIVE LIGHTING PREVIEW", Theme::ACCENT_BRIGHT, Theme::ACCENT_DIM);
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        let active_eff = BACKLIGHT_EFFECTS
                                            .get(backlight.current_effect as usize)
                                            .map(|e| e.name)
                                            .unwrap_or("Static");
                                        pill_badge(ui, active_eff, Theme::SUCCESS_BRIGHT, Theme::SUCCESS_DIM);
                                    });
                                });

                                ui.add_space(6.0);
                                self.render_keyboard_schematic(ui, snapshot);
                                ui.add_space(10.0);

                                ui.label(
                                    RichText::new("Illumination Effects")
                                        .size(14.5)
                                        .strong()
                                        .color(Theme::TEXT_PRIMARY),
                                );
                                ui.label(
                                    RichText::new("Select an onboard lighting animation (or press Fn + FX key).")
                                        .size(11.5)
                                        .color(Theme::TEXT_MUTED),
                                );
                                ui.add_space(8.0);

                                // 2x3 Grid of Effect Cards
                                let mut clicked_effect = None;
                                for pair in BACKLIGHT_EFFECTS.chunks(2) {
                                    ui.horizontal(|ui| {
                                        for effect in pair {
                                            let is_active = backlight.current_effect == effect.id;
                                            let (rect, resp) = ui.allocate_exact_size(Vec2::new(176.0, 68.0), Sense::click());
                                            if resp.hovered() {
                                                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                            }
                                            if resp.clicked() && !is_active {
                                                clicked_effect = Some(effect.id);
                                            }

                                            if ui.is_rect_visible(rect) {
                                                let painter = ui.painter();
                                                let bg = if is_active {
                                                    Theme::BG_SELECTED
                                                } else if resp.hovered() {
                                                    Theme::BG_HOVER
                                                } else {
                                                    Theme::BG_CARD_ALT
                                                };
                                                let border = if is_active {
                                                    Stroke::new(1.5, Theme::ACCENT)
                                                } else if resp.hovered() {
                                                    Stroke::new(1.0, Theme::BORDER_HOVER)
                                                } else {
                                                    Stroke::new(1.0, Theme::BORDER_SUBTLE)
                                                };
                                                painter.rect_filled(rect, CornerRadius::same(9), bg);
                                                painter.rect_stroke(rect, CornerRadius::same(9), border, StrokeKind::Inside);

                                                painter.text(
                                                    Pos2::new(rect.min.x + 12.0, rect.min.y + 18.0),
                                                    egui::Align2::LEFT_CENTER,
                                                    effect.name,
                                                    FontId::proportional(13.5),
                                                    Theme::TEXT_PRIMARY,
                                                );
                                                if is_active {
                                                    painter.circle_filled(
                                                        Pos2::new(rect.max.x - 14.0, rect.min.y + 18.0),
                                                        4.0,
                                                        Theme::ACCENT_BRIGHT,
                                                    );
                                                }
                                                painter.text(
                                                    Pos2::new(rect.min.x + 12.0, rect.min.y + 42.0),
                                                    egui::Align2::LEFT_CENTER,
                                                    effect.desc,
                                                    FontId::proportional(10.0),
                                                    Theme::TEXT_MUTED,
                                                );
                                            }
                                        }
                                    });
                                }

                                if let Some(eff) = clicked_effect {
                                    self.set_backlight_effect(eff);
                                }
                            });
                        });

                    ui.add_space(12.0);

                    // Right Column: Illumination Power, Brightness & Timers
                    ui.vertical(|ui| {
                        // 1. Master Power & Smart Ambient Sensor Card
                        egui::Frame::new()
                            .fill(Theme::BG_CARD)
                            .stroke(Stroke::new(1.0, Theme::BORDER_SUBTLE))
                            .corner_radius(CornerRadius::same(12))
                            .inner_margin(Margin::same(16))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.vertical(|ui| {
                                        ui.label(
                                            RichText::new("Backlighting Power")
                                                .size(14.5)
                                                .strong()
                                                .color(Theme::TEXT_PRIMARY),
                                        );
                                        ui.label(
                                            RichText::new("Master switch for keyboard LED illumination.")
                                                .size(11.5)
                                                .color(Theme::TEXT_MUTED),
                                        );
                                    });
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if toggle_switch(ui, backlight.enabled).clicked() {
                                            self.set_backlight_level(
                                                backlight.level,
                                                backlight.manual_mode,
                                                !backlight.enabled,
                                            );
                                        }
                                    });
                                });

                                ui.add_space(10.0);
                                ui.separator();
                                ui.add_space(10.0);

                                // Automatic Ambient Sensor Toggle
                                let is_auto = !backlight.manual_mode;
                                ui.horizontal(|ui| {
                                    ui.vertical(|ui| {
                                        ui.label(
                                            RichText::new("Smart Ambient Light Sensor (Auto)")
                                                .size(14.5)
                                                .strong()
                                                .color(Theme::TEXT_PRIMARY),
                                        );
                                        ui.label(
                                            RichText::new("Automatically adapts key brightness to room lighting conditions.")
                                                .size(11.5)
                                                .color(Theme::TEXT_MUTED),
                                        );
                                    });
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if toggle_switch(ui, is_auto).clicked() {
                                            self.set_backlight_level(backlight.level, is_auto, backlight.enabled);
                                        }
                                    });
                                });
                            });

                        ui.add_space(10.0);

                        // 2. Manual Brightness Level Card
                        egui::Frame::new()
                            .fill(Theme::BG_CARD)
                            .stroke(Stroke::new(1.0, Theme::BORDER_SUBTLE))
                            .corner_radius(CornerRadius::same(12))
                            .inner_margin(Margin::same(16))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new("Brightness Intensity")
                                            .size(14.5)
                                            .strong()
                                            .color(Theme::TEXT_PRIMARY),
                                    );
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        pill_badge(
                                            ui,
                                            &format!("LEVEL {} / 7", backlight.level),
                                            Theme::ACCENT_BRIGHT,
                                            Theme::ACCENT_DIM,
                                        );
                                    });
                                });
                                ui.add_space(8.0);

                                let mut lvl = backlight.level;
                                if ui
                                    .add(egui::Slider::new(&mut lvl, 0..=7).text("Intensity"))
                                    .changed()
                                {
                                    self.set_backlight_level(lvl, true, backlight.enabled);
                                }

                                ui.add_space(6.0);
                                ui.horizontal(|ui| {
                                    for preset in 0..=7u8 {
                                        let is_curr = backlight.manual_mode && backlight.level == preset;
                                        if ui
                                            .selectable_label(
                                                is_curr,
                                                RichText::new(format!("L{preset}")).size(11.5).strong(),
                                            )
                                            .clicked()
                                        {
                                            self.set_backlight_level(preset, true, backlight.enabled);
                                        }
                                    }
                                });
                            });

                        ui.add_space(10.0);

                        // 3. Sleep Timeout Duration & Battery Saver Card
                        egui::Frame::new()
                            .fill(Theme::BG_CARD)
                            .stroke(Stroke::new(1.0, Theme::BORDER_SUBTLE))
                            .corner_radius(CornerRadius::same(12))
                            .inner_margin(Margin::same(16))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new("Inactivity Sleep Timeout")
                                            .size(14.5)
                                            .strong()
                                            .color(Theme::TEXT_PRIMARY),
                                    );
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        pill_badge(
                                            ui,
                                            &format!("{} SEC", self.backlight_duration_secs),
                                            Theme::ACCENT_BRIGHT,
                                            Theme::ACCENT_DIM,
                                        );
                                    });
                                });
                                ui.label(
                                    RichText::new("Turns off backlighting when hands leave the keyboard.")
                                        .size(11.5)
                                        .color(Theme::TEXT_MUTED),
                                );
                                ui.add_space(8.0);

                                let mut dur = self.backlight_duration_secs;
                                if ui
                                    .add(egui::Slider::new(&mut dur, 5..=300).text("seconds"))
                                    .changed()
                                {
                                    self.set_backlight_duration(dur);
                                }

                                ui.add_space(6.0);
                                ui.horizontal(|ui| {
                                    for preset in [5u16, 10, 30, 60, 120, 180, 300] {
                                        let is_curr = self.backlight_duration_secs == preset;
                                        let label = if preset >= 60 {
                                            format!("{}m", preset / 60)
                                        } else {
                                            format!("{preset}s")
                                        };
                                        if ui
                                            .selectable_label(is_curr, RichText::new(label).size(11.5).strong())
                                            .clicked()
                                        {
                                            self.set_backlight_duration(preset);
                                        }
                                    }
                                });

                                ui.add_space(10.0);
                                ui.separator();
                                ui.add_space(10.0);

                                // Low Battery Auto-Off (<10%) + Factory Reset
                                ui.horizontal(|ui| {
                                    ui.vertical(|ui| {
                                        ui.label(
                                            RichText::new("Low-Battery Auto Cut-off (<10%)")
                                                .size(13.5)
                                                .strong()
                                                .color(Theme::TEXT_PRIMARY),
                                        );
                                        ui.label(
                                            RichText::new("Automatically disables LEDs when battery drops under 10%.")
                                                .size(11.0)
                                                .color(Theme::TEXT_MUTED),
                                        );
                                    });
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if toggle_switch(ui, self.backlight_battery_saving).clicked() {
                                            self.set_backlight_battery_saving(!self.backlight_battery_saving);
                                        }
                                    });
                                });

                                ui.add_space(10.0);
                                ui.horizontal(|ui| {
                                    if toolbar_button(ui, IconKind::Reset, "Restore Default Lighting", false) {
                                        self.reset_backlighting();
                                    }
                                });
                            });
                    });
                });
            });
    }

    // ========================================================================
    // Tab 5: Keyboard Mode (Fn-Lock & Gaming Key Locks)
    // ========================================================================

    fn render_keyboard_settings_tab(&mut self, ui: &mut egui::Ui, snapshot: &Snapshot) {
        if let Some(fn_inverted) = snapshot.fn_inverted {
            egui::Frame::new()
                .fill(Theme::BG_CARD)
                .stroke(Stroke::new(1.0, Theme::BORDER_SUBTLE))
                .corner_radius(CornerRadius::same(12))
                .inner_margin(Margin::same(18))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new("Function Row Behavior (Fn-Lock)")
                            .size(16.0)
                            .strong()
                            .color(Theme::TEXT_PRIMARY),
                    );
                    ui.label(
                        RichText::new("Choose whether pressing F1–F12 triggers standard function keys or media shortcuts by default.")
                            .size(12.0)
                            .color(Theme::TEXT_MUTED),
                    );
                    ui.add_space(14.0);

                    let avail_w = ui.available_width();
                    let card_w = ((avail_w - 12.0) * 0.5).clamp(220.0, 420.0);
                    let mut clicked_fn = None;

                    ui.horizontal(|ui| {
                        let options = [
                            (
                                true,
                                "Standard Function Keys (F1 – F12)",
                                "Developer & Gaming Mode",
                                "F1–F12 send standard function keys. Hold Fn to control media and brightness.",
                            ),
                            (
                                false,
                                "Media & Special Actions",
                                "Productivity Mode",
                                "F1–F12 control volume, playback, and illumination directly. Hold Fn for F1–F12.",
                            ),
                        ];

                        for (inv_val, title, sub, desc) in options {
                            let is_active = fn_inverted == inv_val;
                            let (rect, resp) = ui.allocate_exact_size(Vec2::new(card_w, 96.0), Sense::click());
                            if resp.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                            }
                            if resp.clicked() && !is_active {
                                clicked_fn = Some(inv_val);
                            }

                            if ui.is_rect_visible(rect) {
                                let painter = ui.painter();
                                let bg = if is_active {
                                    Theme::BG_SELECTED
                                } else if resp.hovered() {
                                    Theme::BG_HOVER
                                } else {
                                    Theme::BG_CARD_ALT
                                };
                                let border = if is_active {
                                    Stroke::new(1.5, Theme::ACCENT)
                                } else if resp.hovered() {
                                    Stroke::new(1.0, Theme::BORDER_HOVER)
                                } else {
                                    Stroke::new(1.0, Theme::BORDER_SUBTLE)
                                };
                                painter.rect_filled(rect, CornerRadius::same(10), bg);
                                painter.rect_stroke(rect, CornerRadius::same(10), border, StrokeKind::Inside);

                                painter.circle_filled(
                                    Pos2::new(rect.min.x + 18.0, rect.min.y + 24.0),
                                    4.5,
                                    if is_active { Theme::ACCENT_BRIGHT } else { Theme::TEXT_MUTED },
                                );
                                painter.text(
                                    Pos2::new(rect.min.x + 30.0, rect.min.y + 24.0),
                                    egui::Align2::LEFT_CENTER,
                                    title,
                                    FontId::proportional(14.5),
                                    Theme::TEXT_PRIMARY,
                                );
                                painter.text(
                                    Pos2::new(rect.min.x + 16.0, rect.min.y + 48.0),
                                    egui::Align2::LEFT_CENTER,
                                    sub,
                                    FontId::proportional(11.5),
                                    Theme::ACCENT_BRIGHT,
                                );
                                painter.text(
                                    Pos2::new(rect.min.x + 16.0, rect.min.y + 70.0),
                                    egui::Align2::LEFT_CENTER,
                                    desc,
                                    FontId::proportional(11.0),
                                    Theme::TEXT_MUTED,
                                );
                            }
                        }
                    });

                    if let Some(target) = clicked_fn {
                        self.set_fn_inversion(target);
                    }
                });

            ui.add_space(14.0);
        }

        // Disabled Nuisance Keys (Game Mode Locks)
        if let Some(disabled) = &snapshot.disabled_keys {
            egui::Frame::new()
                .fill(Theme::BG_CARD)
                .stroke(Stroke::new(1.0, Theme::BORDER_SUBTLE))
                .corner_radius(CornerRadius::same(12))
                .inner_margin(Margin::same(18))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new("Game Mode Key Locks (0x4521)")
                            .size(16.0)
                            .strong()
                            .color(Theme::TEXT_PRIMARY),
                    );
                    ui.label(
                        RichText::new("Disable accidental key presses directly in keyboard firmware during fullscreen gaming.")
                            .size(12.0)
                            .color(Theme::TEXT_MUTED),
                    );
                    ui.add_space(14.0);

                    let lock_keys = [
                        (3u8, "Windows / Start", "Prevents accidental minimizing", disabled.win_key),
                        (0u8, "Caps Lock", "Prevents accidental uppercase lock", disabled.caps_lock),
                        (4u8, "Insert", "Prevents overwrite text mode", disabled.insert),
                        (1u8, "Scroll Lock", "Disables legacy scroll lock", disabled.scroll_lock),
                        (2u8, "Num Lock", "Locks numeric keypad state", disabled.num_lock),
                    ];

                    let mut toggled_bit = None;
                    for (bit, title, desc, is_locked) in lock_keys {
                        let bg = if is_locked { Theme::DANGER_DIM } else { Theme::BG_CARD_ALT };
                        let border = if is_locked {
                            Stroke::new(1.0, Theme::DANGER)
                        } else {
                            Stroke::new(1.0, Theme::BORDER_SUBTLE)
                        };

                        egui::Frame::new()
                            .fill(bg)
                            .stroke(border)
                            .corner_radius(CornerRadius::same(9))
                            .inner_margin(Margin::symmetric(14, 10))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    let (ic_rect, _) = ui.allocate_exact_size(Vec2::splat(26.0), Sense::hover());
                                    draw_icon(
                                        ui.painter(),
                                        ic_rect.center(),
                                        15.0,
                                        if is_locked { IconKind::Lock } else { IconKind::Unlock },
                                        if is_locked { Theme::DANGER_BRIGHT } else { Theme::TEXT_SECONDARY },
                                    );

                                    ui.vertical(|ui| {
                                        ui.horizontal(|ui| {
                                            ui.label(
                                                RichText::new(title)
                                                    .size(13.5)
                                                    .strong()
                                                    .color(Theme::TEXT_PRIMARY),
                                            );
                                            if is_locked {
                                                pill_badge(ui, "DISABLED ON HARDWARE", Theme::DANGER_BRIGHT, Theme::BG_ELEVATED);
                                            }
                                        });
                                        ui.label(RichText::new(desc).size(11.0).color(Theme::TEXT_MUTED));
                                    });

                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if toggle_switch(ui, is_locked).clicked() {
                                            toggled_bit = Some(bit);
                                        }
                                    });
                                });
                            });
                        ui.add_space(6.0);
                    }

                    if let Some(bit) = toggled_bit {
                        self.toggle_disabled_key(bit);
                    }
                });
        }
    }

    // ========================================================================
    // Tab 6: Easy-Switch Multi-Host Management
    // ========================================================================

    fn render_easy_switch_tab(&mut self, ui: &mut egui::Ui, snapshot: &Snapshot) {
        let Some(hosts) = &snapshot.hosts else {
            ui.label("Easy-Switch is not supported on this device.");
            return;
        };

        egui::Frame::new()
            .fill(Theme::BG_CARD)
            .stroke(Stroke::new(1.0, Theme::BORDER_SUBTLE))
            .corner_radius(CornerRadius::same(12))
            .inner_margin(Margin::same(18))
            .show(ui, |ui| {
                ui.label(
                    RichText::new("Easy-Switch Multi-Host Channels")
                        .size(16.0)
                        .strong()
                        .color(Theme::TEXT_PRIMARY),
                );
                ui.label(
                    RichText::new("Switch active wireless connection across up to 3 paired computers or devices.")
                        .size(12.0)
                        .color(Theme::TEXT_MUTED),
                );
                ui.add_space(16.0);

                let mut switch_to = None;
                ui.horizontal(|ui| {
                    for slot in &hosts.slots {
                        let card_bg = if slot.paired { Theme::BG_CARD_ALT } else { Theme::BG_CANVAS };
                        let border = if slot.paired {
                            Stroke::new(1.0, Theme::BORDER_CARD)
                        } else {
                            Stroke::new(1.0, Theme::BORDER_SUBTLE)
                        };

                        egui::Frame::new()
                            .fill(card_bg)
                            .stroke(border)
                            .corner_radius(CornerRadius::same(10))
                            .inner_margin(Margin::same(18))
                            .show(ui, |ui| {
                                ui.set_width(210.0);
                                ui.vertical(|ui| {
                                    ui.horizontal(|ui| {
                                        pill_badge(
                                            ui,
                                            &format!("CHANNEL {}", slot.index),
                                            if slot.paired { Theme::ACCENT_BRIGHT } else { Theme::TEXT_MUTED },
                                            if slot.paired { Theme::ACCENT_DIM } else { Theme::BG_ELEVATED },
                                        );
                                        if slot.paired {
                                            pill_badge(ui, "PAIRED", Theme::SUCCESS_BRIGHT, Theme::SUCCESS_DIM);
                                        } else {
                                            pill_badge(ui, "EMPTY", Theme::TEXT_MUTED, Theme::BG_ELEVATED);
                                        }
                                    });
                                    ui.add_space(10.0);
                                    ui.label(
                                        RichText::new(format!("Host Slot {}", slot.index))
                                            .size(16.0)
                                            .strong()
                                            .color(Theme::TEXT_PRIMARY),
                                    );
                                    ui.label(
                                        RichText::new(if slot.paired {
                                            "Ready for instant wireless handoff."
                                        } else {
                                            "No receiver or Bluetooth host paired."
                                        })
                                        .size(11.5)
                                        .color(Theme::TEXT_MUTED),
                                    );
                                    ui.add_space(12.0);
                                    if slot.paired
                                        && toolbar_button(ui, IconKind::Hosts, "Switch to Channel", true)
                                    {
                                        switch_to = Some(slot.index.saturating_sub(1));
                                    }
                                });
                            });
                        ui.add_space(10.0);
                    }
                });

                if let Some(host_idx) = switch_to {
                    self.switch_host(host_idx);
                }
            });
    }

    // ========================================================================
    // Tab 7: Unified Hardware Diagnostics & Searchable HID++ Feature Catalog
    // ========================================================================

    fn render_diagnostics_tab(&mut self, ui: &mut egui::Ui, snapshot: &Snapshot) {
        egui::ScrollArea::vertical()
            .id_salt("diagnostics_scroll")
            .show(ui, |ui| {
                // Top Row: 2 Summary Cards (Hardware Identity + Firmware Entities)
                ui.horizontal_top(|ui| {
                    let half_w = ((ui.available_width() - 12.0) * 0.5).max(260.0);

                    // Identity & Bus Card
                    egui::Frame::new()
                        .fill(Theme::BG_CARD)
                        .stroke(Stroke::new(1.0, Theme::BORDER_SUBTLE))
                        .corner_radius(CornerRadius::same(12))
                        .inner_margin(Margin::same(16))
                        .show(ui, |ui| {
                            ui.set_width(half_w);
                            ui.label(
                                RichText::new("BUS & HARDWARE IDENTITY")
                                    .size(10.0)
                                    .color(Theme::ACCENT_BRIGHT)
                                    .strong(),
                            );
                            ui.add_space(8.0);
                            egui::Grid::new("hw_identity_grid")
                                .num_columns(2)
                                .spacing([18.0, 6.0])
                                .show(ui, |ui| {
                                    ui.label(RichText::new("Product ID:").color(Theme::TEXT_MUTED));
                                    ui.label(RichText::new(format!("0x{}", snapshot.device.product_id)).strong());
                                    ui.end_row();

                                    ui.label(RichText::new("Unit ID:").color(Theme::TEXT_MUTED));
                                    ui.label(RichText::new(&snapshot.device.unit_id).strong());
                                    ui.end_row();

                                    if let Some(serial) = &snapshot.serial {
                                        ui.label(RichText::new("Serial Number:").color(Theme::TEXT_MUTED));
                                        ui.label(RichText::new(serial).strong());
                                        ui.end_row();
                                    }

                                    ui.label(RichText::new("Device Index:").color(Theme::TEXT_MUTED));
                                    ui.label(RichText::new(format!("Slot {}", snapshot.device.index)).strong());
                                    ui.end_row();

                                    if let Some(battery) = &snapshot.battery {
                                        ui.label(RichText::new("Battery State:").color(Theme::TEXT_MUTED));
                                        let v_txt = battery
                                            .voltage_mv
                                            .map(|v| format!(" ({v} mV)"))
                                            .unwrap_or_default();
                                        ui.label(
                                            RichText::new(format!("{}{}", battery.text, v_txt))
                                                .color(Theme::SUCCESS_BRIGHT)
                                                .strong(),
                                        );
                                        ui.end_row();
                                    }
                                });
                        });

                    ui.add_space(12.0);

                    // Microcontroller Firmware Revisions Card
                    egui::Frame::new()
                        .fill(Theme::BG_CARD)
                        .stroke(Stroke::new(1.0, Theme::BORDER_SUBTLE))
                        .corner_radius(CornerRadius::same(12))
                        .inner_margin(Margin::same(16))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(
                                RichText::new("SUBSYSTEM FIRMWARE REVISIONS")
                                    .size(10.0)
                                    .color(Theme::ACCENT_BRIGHT)
                                    .strong(),
                            );
                            ui.add_space(8.0);
                            if snapshot.firmware.is_empty() {
                                ui.label(RichText::new("No firmware entities reported.").color(Theme::TEXT_MUTED));
                            } else {
                                ui.horizontal_wrapped(|ui| {
                                    for fw in &snapshot.firmware {
                                        egui::Frame::new()
                                            .fill(Theme::BG_CARD_ALT)
                                            .stroke(Stroke::new(1.0, Theme::BORDER_SUBTLE))
                                            .corner_radius(CornerRadius::same(8))
                                            .inner_margin(Margin::symmetric(12, 8))
                                            .show(ui, |ui| {
                                                ui.label(
                                                    RichText::new(&fw.name)
                                                        .size(10.5)
                                                        .color(Theme::TEXT_MUTED)
                                                        .strong(),
                                                );
                                                ui.label(
                                                    RichText::new(&fw.version)
                                                        .size(13.5)
                                                        .color(Theme::TEXT_PRIMARY)
                                                        .strong(),
                                                );
                                            });
                                    }
                                });
                            }
                        });
                });

                ui.add_space(14.0);

                // Searchable HID++ 2.0 Feature Table
                egui::Frame::new()
                    .fill(Theme::BG_CARD)
                    .stroke(Stroke::new(1.0, Theme::BORDER_SUBTLE))
                    .corner_radius(CornerRadius::same(12))
                    .inner_margin(Margin::same(16))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.label(
                                    RichText::new("HID++ 2.0 Feature Register")
                                        .size(15.5)
                                        .strong()
                                        .color(Theme::TEXT_PRIMARY),
                                );
                                ui.label(
                                    RichText::new(format!(
                                        "{} active protocol features exposed by hardware.",
                                        snapshot.device.features.len()
                                    ))
                                    .size(11.5)
                                    .color(Theme::TEXT_MUTED),
                                );
                            });
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.feature_search)
                                        .hint_text("Filter features (e.g. 1982, Battery, Onboard)...")
                                        .desired_width(240.0),
                                );
                            });
                        });

                        ui.add_space(10.0);
                        ui.separator();
                        ui.add_space(6.0);

                        let f_query = self.feature_search.trim().to_lowercase();
                        egui::Grid::new("features_catalog_grid")
                            .striped(true)
                            .num_columns(3)
                            .spacing([28.0, 7.0])
                            .show(ui, |ui| {
                                ui.label(RichText::new("INDEX").size(10.5).color(Theme::TEXT_MUTED).strong());
                                ui.label(RichText::new("FEATURE ID").size(10.5).color(Theme::TEXT_MUTED).strong());
                                ui.label(RichText::new("PROTOCOL CAPABILITY").size(10.5).color(Theme::TEXT_MUTED).strong());
                                ui.end_row();

                                for feat in &snapshot.device.features {
                                    if !f_query.is_empty() {
                                        let hex = format!("{:04x}", feat.id);
                                        if !feat.name.to_lowercase().contains(&f_query) && !hex.contains(&f_query) {
                                            continue;
                                        }
                                    }
                                    ui.label(
                                        RichText::new(format!("0x{:02X}", feat.index))
                                            .color(Theme::TEXT_MUTED)
                                            .size(12.0),
                                    );
                                    ui.label(
                                        RichText::new(format!("0x{:04X}", feat.id))
                                            .color(Theme::ACCENT_BRIGHT)
                                            .strong()
                                            .size(12.0),
                                    );
                                    ui.label(
                                        RichText::new(&feat.name)
                                            .color(Theme::TEXT_PRIMARY)
                                            .strong()
                                            .size(12.5),
                                    );
                                    ui.end_row();
                                }
                            });
                    });
            });
    }

    // ========================================================================
    // Preferences Modal Window
    // ========================================================================

    fn render_settings_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_settings_modal;
        let mut close_modal = false;
        egui::Window::new("Preferences")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_size([480.0, 340.0])
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(Theme::BG_CARD)
                    .stroke(Stroke::new(1.0, Theme::BORDER_SUBTLE))
                    .corner_radius(CornerRadius::same(10))
                    .inner_margin(Margin::same(14))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.label(
                                    RichText::new("Launch at Windows Startup")
                                        .size(14.5)
                                        .strong()
                                        .color(Theme::TEXT_PRIMARY),
                                );
                                ui.label(
                                    RichText::new("Start minimized in the notification tray and restore hardware profiles on boot.")
                                        .size(11.5)
                                        .color(Theme::TEXT_MUTED),
                                );
                            });
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if toggle_switch(ui, self.autostart_enabled).clicked() {
                                    self.autostart_enabled = !self.autostart_enabled;
                                    let _ = crate::autostart::set_enabled(self.autostart_enabled);
                                    self.set_status(
                                        if self.autostart_enabled {
                                            "Windows startup launch enabled"
                                        } else {
                                            "Windows startup launch disabled"
                                        },
                                        false,
                                    );
                                }
                            });
                        });
                    });

                ui.add_space(10.0);

                egui::Frame::new()
                    .fill(Theme::BG_CARD)
                    .stroke(Stroke::new(1.0, Theme::BORDER_SUBTLE))
                    .corner_radius(CornerRadius::same(10))
                    .inner_margin(Margin::same(14))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new("Background & Memory Efficiency")
                                .size(14.5)
                                .strong()
                                .color(Theme::TEXT_PRIMARY),
                        );
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            pill_badge(ui, "~10 MB RAM", Theme::SUCCESS_BRIGHT, Theme::SUCCESS_DIM);
                            pill_badge(ui, "0.0% Idle CPU", Theme::ACCENT_BRIGHT, Theme::ACCENT_DIM);
                            pill_badge(ui, "Native Rust + OpenGL", Theme::TEXT_SECONDARY, Theme::BG_ELEVATED);
                        });
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new("Closing the main window hides betterTech to your system tray and trims the OS working set automatically.")
                                .size(11.5)
                                .color(Theme::TEXT_MUTED),
                        );
                    });

                ui.add_space(12.0);

                ui.horizontal(|ui| {
                    if let Ok(path) = config::path() {
                        ui.label(
                            RichText::new(format!("Config: {}", path.display()))
                                .size(10.5)
                                .color(Theme::TEXT_MUTED),
                        );
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if toolbar_button(ui, IconKind::Check, "Done", true) {
                            close_modal = true;
                        }
                    });
                });
            });

        if close_modal {
            open = false;
        }
        self.show_settings_modal = open;
    }
}

// ============================================================================
// Keycap Painter Helpers
// ============================================================================

fn draw_keycap(painter: &egui::Painter, rect: Rect, is_modifier: bool, glow_alpha: u8) {
    let base_bg = if is_modifier {
        Color32::from_rgb(25, 31, 43)
    } else {
        Color32::from_rgb(33, 41, 56)
    };
    if glow_alpha > 0 {
        let bloom = Color32::from_rgba_premultiplied(14, 165, 233, (glow_alpha / 6).min(38));
        painter.rect_filled(rect.expand(2.0), CornerRadius::same(4), bloom);

        let halo = Color32::from_rgba_premultiplied(56, 189, 248, (glow_alpha / 3).min(85));
        painter.rect_stroke(
            rect.expand(0.4),
            CornerRadius::same(3),
            Stroke::new(1.0, halo),
            StrokeKind::Outside,
        );
    }
    let cap_color = if glow_alpha > 120 {
        Color32::from_rgb(44, 54, 72)
    } else {
        base_bg
    };
    painter.rect_filled(rect, CornerRadius::same(3), cap_color);
    let border_color = if glow_alpha > 80 {
        Color32::from_rgba_premultiplied(125, 211, 252, (glow_alpha / 2).max(40))
    } else {
        Color32::from_rgb(48, 60, 80)
    };
    painter.rect_stroke(rect, CornerRadius::same(3), Stroke::new(0.7, border_color), StrokeKind::Inside);
}

fn draw_keycap_accent(painter: &egui::Painter, rect: Rect, label: &str, accent_color: Color32) {
    painter.rect_filled(
        rect.expand(1.5),
        CornerRadius::same(4),
        Color32::from_rgba_premultiplied(accent_color.r(), accent_color.g(), accent_color.b(), 28),
    );
    painter.rect_filled(rect, CornerRadius::same(3), Color32::from_rgb(22, 30, 44));
    painter.rect_stroke(rect, CornerRadius::same(3), Stroke::new(1.1, accent_color), StrokeKind::Inside);
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        FontId::proportional(9.0),
        accent_color,
    );
}
