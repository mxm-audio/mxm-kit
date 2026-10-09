//! A top-level window for a floating editor: VST3 views only ever attach to a window the host
//! gives them, so a host that asks CLAP for a floating editor gets one of these, with the view
//! attached inside.
//!
//! Each platform's window is the plainest it has: a Win32 window, an `NSWindow`, an X11 window.
//! Its user's resizing and closing come back as [`WindowEvent`]s on the main thread.

/// What the window's user did.
#[derive(Clone, Copy, Debug)]
pub(crate) enum WindowEvent {
    /// The client area is now this size, in the platform's units (pixels on Windows and X11,
    /// points on macOS, as CLAP's `gui` uses them).
    Resized(u32, u32),
    /// The close button: the window is hidden, never destroyed by its user.
    CloseRequested,
}

pub(crate) type EventHandler = Box<dyn FnMut(WindowEvent)>;

#[cfg(target_os = "windows")]
mod platform {
    use super::{EventHandler, WindowEvent};
    use std::cell::RefCell;
    use std::ffi::c_void;
    use std::sync::OnceLock;
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        AdjustWindowRectEx, CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, CreateWindowExW,
        DefWindowProcW, DestroyWindow, GWL_STYLE, GWLP_HWNDPARENT, GWLP_USERDATA,
        GetWindowLongPtrW, IDC_ARROW, LoadCursorW, RegisterClassExW, SIZE_MINIMIZED, SW_HIDE,
        SW_SHOW, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOZORDER, SetForegroundWindow, SetWindowLongPtrW,
        SetWindowPos, SetWindowTextW, ShowWindow, WM_CLOSE, WM_NCCREATE, WM_SIZE, WNDCLASSEXW,
        WS_CAPTION, WS_CLIPCHILDREN, WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_OVERLAPPED, WS_SYSMENU,
        WS_THICKFRAME,
    };

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// The window class, registered once per process.
    fn class_name() -> *const u16 {
        static CLASS: OnceLock<Vec<u16>> = OnceLock::new();
        CLASS
            .get_or_init(|| {
                let name = wide("MxmVst3EditorWindow");
                // SAFETY: a plain class registration with a static window procedure.
                unsafe {
                    let class = WNDCLASSEXW {
                        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                        style: CS_HREDRAW | CS_VREDRAW,
                        lpfnWndProc: Some(window_procedure),
                        cbClsExtra: 0,
                        cbWndExtra: 0,
                        hInstance: GetModuleHandleW(std::ptr::null()),
                        hIcon: std::ptr::null_mut(),
                        hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
                        hbrBackground: std::ptr::null_mut(),
                        lpszMenuName: std::ptr::null(),
                        lpszClassName: name.as_ptr(),
                        hIconSm: std::ptr::null_mut(),
                    };
                    RegisterClassExW(&class);
                }
                name
            })
            .as_ptr()
    }

    struct State {
        on_event: RefCell<EventHandler>,
    }

    impl State {
        fn send(&self, event: WindowEvent) {
            // A resize the handler itself causes arrives while it runs: it is the handler's own.
            if let Ok(mut on_event) = self.on_event.try_borrow_mut() {
                on_event(event);
            }
        }
    }

    unsafe extern "system" fn window_procedure(
        window: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        // SAFETY: Win32's contract for a window procedure; the state pointer is the one `new`
        // created, cleared before it is freed.
        unsafe {
            if message == WM_NCCREATE {
                let create = &*(lparam as *const CREATESTRUCTW);
                SetWindowLongPtrW(window, GWLP_USERDATA, create.lpCreateParams as isize);
                return DefWindowProcW(window, message, wparam, lparam);
            }
            let state = GetWindowLongPtrW(window, GWLP_USERDATA) as *const State;
            if let Some(state) = state.as_ref() {
                match message {
                    WM_SIZE => {
                        if wparam as u32 != SIZE_MINIMIZED {
                            let width = (lparam & 0xFFFF) as u32;
                            let height = ((lparam >> 16) & 0xFFFF) as u32;
                            state.send(WindowEvent::Resized(width, height));
                        }
                        return 0;
                    }
                    WM_CLOSE => {
                        state.send(WindowEvent::CloseRequested);
                        return 0;
                    }
                    _ => {}
                }
            }
            DefWindowProcW(window, message, wparam, lparam)
        }
    }

    /// What resizing and showing need: the window's handle.
    #[derive(Clone, Copy)]
    pub(crate) struct Handle(HWND);

    impl Handle {
        pub(crate) fn resize(&self, width: u32, height: u32) {
            // SAFETY: a live window of ours, on its thread.
            unsafe {
                let style = GetWindowLongPtrW(self.0, GWL_STYLE) as u32;
                let mut frame = RECT {
                    left: 0,
                    top: 0,
                    right: width as i32,
                    bottom: height as i32,
                };
                AdjustWindowRectEx(&mut frame, style, 0, 0);
                SetWindowPos(
                    self.0,
                    std::ptr::null_mut(),
                    0,
                    0,
                    frame.right - frame.left,
                    frame.bottom - frame.top,
                    SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
        }

        pub(crate) fn set_visible(&self, visible: bool) {
            // SAFETY: a live window of ours, on its thread.
            unsafe {
                ShowWindow(self.0, if visible { SW_SHOW } else { SW_HIDE });
                if visible {
                    SetForegroundWindow(self.0);
                }
            }
        }
    }

    pub(crate) struct Window {
        window: HWND,
        state: *mut State,
    }

    impl Window {
        pub(crate) fn new(
            title: &str,
            width: u32,
            height: u32,
            resizable: bool,
            on_event: EventHandler,
        ) -> Result<Self, String> {
            let mut style =
                WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_CLIPCHILDREN;
            if resizable {
                style |= WS_THICKFRAME | WS_MAXIMIZEBOX;
            }
            let mut frame = RECT {
                left: 0,
                top: 0,
                right: width as i32,
                bottom: height as i32,
            };
            let state = Box::into_raw(Box::new(State {
                on_event: RefCell::new(on_event),
            }));
            let title = wide(title);
            // SAFETY: creating a hidden window of the registered class, handing it the state.
            let window = unsafe {
                AdjustWindowRectEx(&mut frame, style, 0, 0);
                CreateWindowExW(
                    0,
                    class_name(),
                    title.as_ptr(),
                    style,
                    CW_USEDEFAULT,
                    CW_USEDEFAULT,
                    frame.right - frame.left,
                    frame.bottom - frame.top,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    GetModuleHandleW(std::ptr::null()),
                    state as *const c_void,
                )
            };
            if window.is_null() {
                // SAFETY: the state was never handed to a window.
                drop(unsafe { Box::from_raw(state) });
                return Err("CreateWindowExW failed".into());
            }
            Ok(Self { window, state })
        }

        pub(crate) fn native(&self) -> *mut c_void {
            self.window
        }

        pub(crate) fn handle(&self) -> Handle {
            Handle(self.window)
        }

        pub(crate) fn set_owner(&self, owner: *mut c_void) {
            // SAFETY: a live window of ours; the owner is the host's window.
            unsafe { SetWindowLongPtrW(self.window, GWLP_HWNDPARENT, owner as isize) };
        }

        pub(crate) fn set_title(&self, title: &str) {
            let title = wide(title);
            // SAFETY: a live window of ours; the title is NUL-terminated.
            unsafe { SetWindowTextW(self.window, title.as_ptr()) };
        }
    }

    impl Drop for Window {
        fn drop(&mut self) {
            // SAFETY: the state is detached before the window goes and freed after.
            unsafe {
                SetWindowLongPtrW(self.window, GWLP_USERDATA, 0);
                DestroyWindow(self.window);
                drop(Box::from_raw(self.state));
            }
        }
    }
}

#[cfg(target_os = "macos")]
#[path = "window_macos.rs"]
mod platform;

#[cfg(target_os = "linux")]
#[path = "window_x11.rs"]
mod platform;

pub(crate) use platform::{Handle, Window};
