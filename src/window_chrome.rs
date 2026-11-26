use crate::ui::{BORDER, ToolbarRegions, WindowButtons};
use eframe::egui::{Color32, Context, Rect};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::cell::Cell;
use std::ffi::c_void;
use std::mem::size_of;
use std::ptr::null_mut;
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
    Graphics::{
        Dwm::{DWMWA_BORDER_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE, DwmSetWindowAttribute},
        Gdi::ScreenToClient,
    },
    UI::{
        HiDpi::{AdjustWindowRectExForDpi, GetDpiForWindow, GetSystemMetricsForDpi},
        Input::KeyboardAndMouse::{
            GetCapture, ReleaseCapture, SetCapture, TME_LEAVE, TME_NONCLIENT, TRACKMOUSEEVENT,
            TrackMouseEvent,
        },
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::{
            GWL_EXSTYLE, GWL_STYLE, GetClientRect, GetForegroundWindow, GetWindowLongW,
            GetWindowRect, HTCAPTION, HTCLIENT, HTMAXBUTTON, HTTOP, HTTOPLEFT, HTTOPRIGHT,
            IsZoomed, MINMAXINFO, NCCALCSIZE_PARAMS, PostMessageW, SC_MAXIMIZE, SC_RESTORE,
            SM_CXPADDEDBORDER, SM_CXSIZEFRAME, SM_CYSIZEFRAME, SWP_FRAMECHANGED, SWP_NOACTIVATE,
            SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SetWindowPos, WM_CANCELMODE, WM_CAPTURECHANGED,
            WM_DPICHANGED, WM_GETMINMAXINFO, WM_LBUTTONUP, WM_MOUSEMOVE, WM_NCACTIVATE,
            WM_NCCALCSIZE, WM_NCDESTROY, WM_NCHITTEST, WM_NCLBUTTONDBLCLK, WM_NCLBUTTONDOWN,
            WM_NCMOUSELEAVE, WM_NCMOUSEMOVE, WM_SIZE, WM_SYSCOMMAND,
        },
    },
};

const SUBCLASS_ID: usize = 1;
const ACTIVE_BORDER: Color32 = Color32::from_rgb(86, 82, 75);

#[derive(Clone, Copy, Default)]
struct HitRegions {
    drag: RECT,
    controls: [RECT; 3],
    maximize: RECT,
}

struct FrameState {
    installed: Cell<bool>,
    regions: Cell<HitRegions>,
    maximize_hovered: Cell<bool>,
    maximize_pressed: Cell<bool>,
    context: Context,
}

impl FrameState {
    fn set_maximize_hovered(&self, hovered: bool) -> bool {
        let changed = self.maximize_hovered.replace(hovered) != hovered;
        if changed {
            self.context.request_repaint();
        }
        changed
    }

    fn reset_maximize_press(&self) -> bool {
        let pressed = self.maximize_pressed.replace(false);
        if pressed {
            self.maximize_hovered.set(false);
            self.context.request_repaint();
        }
        pressed
    }
}

pub(crate) struct WindowChrome {
    hwnd: HWND,
    state: Box<FrameState>,
}

impl WindowChrome {
    pub(crate) fn install(creation: &eframe::CreationContext<'_>) -> Option<Self> {
        let RawWindowHandle::Win32(handle) = creation.window_handle().ok()?.as_raw() else {
            return None;
        };
        let hwnd = handle.hwnd.get() as HWND;
        let state = Box::new(FrameState {
            installed: Cell::new(false),
            regions: Cell::default(),
            maximize_hovered: Cell::new(false),
            maximize_pressed: Cell::new(false),
            context: creation.egui_ctx.clone(),
        });

        // eframe creates and updates the HWND on this thread. The boxed state
        // stays at the same address until the subclass is removed or destroyed.
        unsafe {
            if SetWindowSubclass(
                hwnd,
                Some(window_proc),
                SUBCLASS_ID,
                (&*state as *const FrameState) as usize,
            ) == 0
            {
                return None;
            }
            state.installed.set(true);
            let chrome = Self { hwnd, state };
            let mut window = RECT::default();
            let mut flags = SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE;
            if GetWindowRect(hwnd, &mut window) == 0 || IsZoomed(hwnd) != 0 {
                flags |= SWP_NOSIZE;
            }
            if SetWindowPos(
                hwnd,
                null_mut(),
                0,
                0,
                window.right - window.left,
                window.bottom - window.top - standard_top_inset(hwnd),
                flags,
            ) == 0
            {
                return None;
            }
            set_attribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE as u32, 1);
            set_border(hwnd, GetForegroundWindow() == hwnd);
            Some(chrome)
        }
    }

    pub(crate) fn buttons(&self) -> WindowButtons {
        let maximize_hovered = self.state.maximize_hovered.get();
        WindowButtons {
            maximized: unsafe { IsZoomed(self.hwnd) != 0 },
            maximize_hovered,
            maximize_pressed: maximize_hovered && self.state.maximize_pressed.get(),
        }
    }

    pub(crate) fn set_toolbar_regions(&self, regions: ToolbarRegions, pixels_per_point: f32) {
        let to_pixels = |rect: Rect| RECT {
            left: (rect.left() * pixels_per_point).ceil() as i32,
            top: (rect.top() * pixels_per_point).ceil() as i32,
            right: (rect.right() * pixels_per_point).floor() as i32,
            bottom: (rect.bottom() * pixels_per_point).floor() as i32,
        };
        self.state.regions.set(HitRegions {
            drag: to_pixels(regions.drag),
            controls: regions.controls.map(to_pixels),
            maximize: regions.maximize.map_or_else(RECT::default, to_pixels),
        });
    }
}

impl Drop for WindowChrome {
    fn drop(&mut self) {
        if self.state.installed.get() {
            // The eframe event-loop thread owns the subclass and the HWND.
            unsafe { RemoveWindowSubclass(self.hwnd, Some(window_proc), SUBCLASS_ID) };
        }
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    subclass: usize,
    data: usize,
) -> LRESULT {
    // SetWindowSubclass receives a live FrameState from WindowChrome::install.
    // Win32 invokes this callback on the same thread and owns message pointers.
    unsafe {
        let state = &*(data as *const FrameState);
        match message {
            WM_NCDESTROY => {
                state.installed.set(false);
                RemoveWindowSubclass(hwnd, Some(window_proc), subclass);
            }
            WM_NCCALCSIZE if wparam != 0 => {
                let params = lparam as *mut NCCALCSIZE_PARAMS;
                let window_top = (*params).rgrc[0].top;
                DefSubclassProc(hwnd, message, wparam, lparam);
                // Keep the native left, right and bottom insets.  The maximized
                // window also needs the top sizing inset to stay in the work area.
                (*params).rgrc[0].top = window_top
                    + if IsZoomed(hwnd) != 0 {
                        resize_inset(hwnd).1
                    } else {
                        0
                    };
                return 0;
            }
            WM_GETMINMAXINFO => {
                let result = DefSubclassProc(hwnd, message, wparam, lparam);
                let min_size = &mut (*(lparam as *mut MINMAXINFO)).ptMinTrackSize;
                // winit includes the standard caption in its outer track size.
                min_size.y = (min_size.y - standard_top_inset(hwnd)).max(1);
                return result;
            }
            WM_NCACTIVATE => set_border(hwnd, wparam != 0),
            WM_DPICHANGED | WM_SIZE => {
                state.regions.set(HitRegions::default());
                state.set_maximize_hovered(false);
            }
            WM_NCMOUSEMOVE => {
                let over_maximize = wparam == HTMAXBUTTON as WPARAM;
                if state.set_maximize_hovered(over_maximize) && over_maximize {
                    TrackMouseEvent(&mut TRACKMOUSEEVENT {
                        cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE | TME_NONCLIENT,
                        hwndTrack: hwnd,
                        dwHoverTime: 0,
                    });
                }
            }
            WM_NCLBUTTONDOWN | WM_NCLBUTTONDBLCLK if wparam == HTMAXBUTTON as WPARAM => {
                // Capture the click so releasing outside the custom button cancels it.
                SetCapture(hwnd);
                if GetCapture() == hwnd {
                    state.maximize_pressed.set(true);
                    state.maximize_hovered.set(true);
                    state.context.request_repaint();
                    return 0;
                }
            }
            WM_MOUSEMOVE if state.maximize_pressed.get() => {
                state.set_maximize_hovered(contains(
                    state.regions.get().maximize,
                    message_point(lparam),
                ));
                return 0;
            }
            WM_LBUTTONUP if state.reset_maximize_press() => {
                ReleaseCapture();
                if contains(state.regions.get().maximize, message_point(lparam)) {
                    let command = if IsZoomed(hwnd) != 0 {
                        SC_RESTORE
                    } else {
                        SC_MAXIMIZE
                    };
                    PostMessageW(hwnd, WM_SYSCOMMAND, command as WPARAM, 0);
                }
                return 0;
            }
            WM_CAPTURECHANGED => {
                state.reset_maximize_press();
            }
            WM_CANCELMODE => {
                if state.reset_maximize_press() && GetCapture() == hwnd {
                    ReleaseCapture();
                }
            }
            WM_NCMOUSELEAVE | WM_MOUSEMOVE => {
                state.set_maximize_hovered(false);
            }
            _ => {}
        }

        if message == WM_NCHITTEST
            && let Some(hit) = hit_test(hwnd, lparam, state.regions.get())
        {
            return hit;
        }
        DefSubclassProc(hwnd, message, wparam, lparam)
    }
}

fn hit_test(hwnd: HWND, lparam: LPARAM, regions: HitRegions) -> Option<LRESULT> {
    let mut point = message_point(lparam);
    let mut client = RECT::default();
    // Signed screen coordinates also work on monitors left or above the primary.
    unsafe {
        if ScreenToClient(hwnd, &mut point) == 0 || GetClientRect(hwnd, &mut client) == 0 {
            return None;
        }
        // Windows uses the maximize hit code to open the Snap Layouts flyout.
        if contains(regions.maximize, point) {
            return Some(HTMAXBUTTON as LRESULT);
        }
        if IsZoomed(hwnd) == 0 {
            let (horizontal, vertical) = resize_inset(hwnd);
            if (0..vertical).contains(&point.y) {
                if point.x < horizontal {
                    return Some(HTTOPLEFT as LRESULT);
                }
                if point.x >= client.right - horizontal {
                    return Some(HTTOPRIGHT as LRESULT);
                }
                // Preserve the visible button area where it overlaps the
                // system's top resize band.
                if regions.controls.iter().any(|rect| contains(*rect, point)) {
                    return Some(HTCLIENT as LRESULT);
                }
                return Some(HTTOP as LRESULT);
            }
        }
    }
    if contains(regions.drag, point) {
        Some(HTCAPTION as LRESULT)
    } else if contains(client, point) {
        Some(HTCLIENT as LRESULT)
    } else {
        None
    }
}

fn message_point(lparam: LPARAM) -> POINT {
    POINT {
        x: lparam as i16 as i32,
        y: (lparam >> 16) as i16 as i32,
    }
}

fn contains(rect: RECT, point: POINT) -> bool {
    (rect.left..rect.right).contains(&point.x) && (rect.top..rect.bottom).contains(&point.y)
}

fn resize_inset(hwnd: HWND) -> (i32, i32) {
    // The input handle comes from the main window's own subclass callback.
    unsafe {
        let dpi = GetDpiForWindow(hwnd);
        let padding = GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi);
        (
            padding + GetSystemMetricsForDpi(SM_CXSIZEFRAME, dpi),
            padding + GetSystemMetricsForDpi(SM_CYSIZEFRAME, dpi),
        )
    }
}

fn standard_top_inset(hwnd: HWND) -> i32 {
    let mut frame = RECT::default();
    // Use the same style and DPI conversion as winit's client size adjustment.
    unsafe {
        AdjustWindowRectExForDpi(
            &mut frame,
            GetWindowLongW(hwnd, GWL_STYLE) as u32,
            0,
            GetWindowLongW(hwnd, GWL_EXSTYLE) as u32,
            GetDpiForWindow(hwnd),
        );
    }
    -frame.top
}

fn set_border(hwnd: HWND, active: bool) {
    let color = if active { ACTIVE_BORDER } else { BORDER };
    set_attribute(hwnd, DWMWA_BORDER_COLOR as u32, color_ref(color));
}

fn set_attribute(hwnd: HWND, attribute: u32, value: u32) {
    // All attributes used here take a four-byte COLORREF or BOOL. Older build
    // numbers may omit the cosmetic attributes and use their system defaults.
    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            attribute,
            (&value as *const u32).cast::<c_void>(),
            size_of::<u32>() as u32,
        );
    }
}

fn color_ref(color: Color32) -> u32 {
    u32::from_le_bytes([color.r(), color.g(), color.b(), 0])
}
