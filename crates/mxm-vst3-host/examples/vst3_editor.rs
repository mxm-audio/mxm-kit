//! Opens a VST3 plugin's editor through CLAP's `gui` extension, as MXM Player (floating) and
//! newDAWn (embedded, on Windows) do, runs the window for a moment and captures it to a 32-bit
//! BMP, so a check can see the plugin drew.
//!
//! ```text
//! cargo run -p mxm-vst3-host --example vst3_editor -- <path.vst3> <floating|embedded> <out.bmp> [milliseconds]
//! ```
//!
//! Windows only: the capture is Win32's.

#[cfg(not(target_os = "windows"))]
fn main() {
    eprintln!("vst3_editor captures with Win32 and runs on Windows only");
}

#[cfg(target_os = "windows")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    mxm_vst3_host::serve_probe();
    windows::run()
}

#[cfg(target_os = "windows")]
mod windows {
    use clack_extensions::gui::{GuiApiType, GuiConfiguration, PluginGui, Window as ClapWindow};
    use clack_host::prelude::*;
    use std::ffi::{CString, c_void};
    use std::time::{Duration, Instant};
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows_sys::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS,
        DeleteDC, DeleteObject, GetDC, ReleaseDC, SelectObject,
    };
    use windows_sys::Win32::Storage::Xps::{PRINT_WINDOW_FLAGS, PrintWindow};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CW_USEDEFAULT, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
        FindWindowW, GetClientRect, MSG, PM_REMOVE, PeekMessageW, RegisterClassExW, SW_SHOW,
        SWP_NOMOVE, SWP_NOZORDER, SetWindowPos, ShowWindow, TranslateMessage, WNDCLASSEXW,
        WS_CLIPCHILDREN, WS_OVERLAPPEDWINDOW,
    };

    struct Editor;

    impl HostHandlers for Editor {
        type Shared<'a> = ();
        type MainThread<'a> = ();
        type AudioProcessor<'a> = ();
    }

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn pump_for(duration: Duration) {
        let until = Instant::now() + duration;
        while Instant::now() < until {
            // SAFETY: a standard message pump on this thread.
            unsafe {
                let mut message: MSG = std::mem::zeroed();
                while PeekMessageW(&mut message, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                    TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    unsafe extern "system" fn parent_procedure(
        window: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        // SAFETY: the default procedure, for a plain parent window.
        unsafe { DefWindowProcW(window, message, wparam, lparam) }
    }

    /// A plain top-level window, as a host gives an embedded editor.
    fn parent_window() -> HWND {
        let class = wide("MxmVst3EditorProbeParent");
        // SAFETY: registering and creating a plain window.
        unsafe {
            let description = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                style: 0,
                lpfnWndProc: Some(parent_procedure),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: GetModuleHandleW(std::ptr::null()),
                hIcon: std::ptr::null_mut(),
                hCursor: std::ptr::null_mut(),
                hbrBackground: std::ptr::null_mut(),
                lpszMenuName: std::ptr::null(),
                lpszClassName: class.as_ptr(),
                hIconSm: std::ptr::null_mut(),
            };
            RegisterClassExW(&description);
            let title = wide("embedded editor");
            CreateWindowExW(
                0,
                class.as_ptr(),
                title.as_ptr(),
                WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                640,
                480,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                GetModuleHandleW(std::ptr::null()),
                std::ptr::null(),
            )
        }
    }

    /// Captures `window`'s client area as a bottom-up 32-bit BMP.
    fn capture(window: HWND, path: &str) -> Result<(i32, i32), Box<dyn std::error::Error>> {
        // SAFETY: GDI calls on a live window; every object made is deleted.
        unsafe {
            let mut client: RECT = std::mem::zeroed();
            GetClientRect(window, &mut client);
            let (width, height) = (client.right - client.left, client.bottom - client.top);
            if width <= 0 || height <= 0 {
                return Err("the window has no client area".into());
            }
            let screen = GetDC(std::ptr::null_mut());
            let memory = CreateCompatibleDC(screen);
            let mut info: BITMAPINFO = std::mem::zeroed();
            info.bmiHeader = BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..std::mem::zeroed()
            };
            let mut bits: *mut c_void = std::ptr::null_mut();
            let bitmap = CreateDIBSection(
                memory,
                &info,
                DIB_RGB_COLORS,
                &mut bits,
                std::ptr::null_mut(),
                0,
            );
            let previous = SelectObject(memory, bitmap);
            // PW_CLIENTONLY | PW_RENDERFULLCONTENT: the client area, GPU-drawn content included.
            let printed = PrintWindow(window, memory, 1 | 2 as PRINT_WINDOW_FLAGS);
            let pixels =
                std::slice::from_raw_parts(bits as *const u8, (width * height * 4) as usize)
                    .to_vec();
            SelectObject(memory, previous);
            DeleteObject(bitmap);
            DeleteDC(memory);
            ReleaseDC(std::ptr::null_mut(), screen);
            if printed == 0 {
                return Err("PrintWindow failed".into());
            }
            let mut file = Vec::with_capacity(54 + pixels.len());
            file.extend_from_slice(b"BM");
            file.extend_from_slice(&(54 + pixels.len() as u32).to_le_bytes());
            file.extend_from_slice(&0u32.to_le_bytes());
            file.extend_from_slice(&54u32.to_le_bytes());
            file.extend_from_slice(&40u32.to_le_bytes());
            file.extend_from_slice(&width.to_le_bytes());
            file.extend_from_slice(&height.to_le_bytes());
            file.extend_from_slice(&1u16.to_le_bytes());
            file.extend_from_slice(&32u16.to_le_bytes());
            file.extend_from_slice(&[0u8; 24]);
            file.extend_from_slice(&pixels);
            std::fs::write(path, file)?;
            Ok((width, height))
        }
    }

    pub(super) fn run() -> Result<(), Box<dyn std::error::Error>> {
        let mut args = std::env::args().skip(1);
        let usage = "usage: vst3_editor <path.vst3> <floating|embedded> <out.bmp> [milliseconds]";
        let path = args.next().ok_or(usage)?;
        let floating = match args.next().as_deref() {
            Some("floating") => true,
            Some("embedded") => false,
            _ => return Err(usage.into()),
        };
        let out = args.next().ok_or(usage)?;
        let milliseconds: u64 = args
            .next()
            .map(|ms| ms.parse())
            .transpose()?
            .unwrap_or(1500);

        let entry = PluginEntry::load_from_clack::<mxm_vst3_host::Vst3Entry>(&CString::new(path)?)?;
        let factory = entry.get_plugin_factory().ok_or("no plugin factory")?;
        let id = factory
            .plugin_descriptors()
            .find_map(|descriptor| descriptor.id().map(|id| id.to_owned()))
            .ok_or("no plugins")?;
        let host_info = HostInfo::new("mxm-vst3-host editor probe", "mxm", "https://mxm.dk", "0")?;
        let mut instance = PluginInstance::<Editor>::new(|_| (), |_| (), &entry, &id, &host_info)?;
        let gui: PluginGui = instance
            .plugin_shared_handle()
            .get_extension()
            .ok_or("no gui extension")?;
        let configuration = GuiConfiguration {
            api_type: GuiApiType::WIN32,
            is_floating: floating,
        };
        let mut handle = instance.plugin_handle();
        if !gui.is_api_supported(&mut handle, configuration) {
            return Err("the editor does not support this configuration".into());
        }
        gui.create(&mut handle, configuration)?;
        let size = gui.get_size(&mut handle).ok_or("the editor has no size")?;
        println!(
            "editor size {}x{}, resizable {}",
            size.width,
            size.height,
            gui.can_resize(&mut handle)
        );

        let parent = if floating {
            gui.suggest_title(&mut handle, c"floating editor");
            gui.show(&mut handle)?;
            None
        } else {
            let parent = parent_window();
            // SAFETY: a live window of this thread, sized to the editor's client area plus frame.
            unsafe {
                let frame_extra = (16, 39);
                SetWindowPos(
                    parent,
                    std::ptr::null_mut(),
                    0,
                    0,
                    size.width as i32 + frame_extra.0,
                    size.height as i32 + frame_extra.1,
                    SWP_NOMOVE | SWP_NOZORDER,
                );
                ShowWindow(parent, SW_SHOW);
                gui.set_parent(&mut handle, ClapWindow::from_win32_hwnd(parent))?;
            }
            gui.show(&mut handle)?;
            Some(parent)
        };
        pump_for(Duration::from_millis(milliseconds));

        let window = match parent {
            Some(parent) => parent,
            None => {
                let class = wide("MxmVst3EditorWindow");
                // SAFETY: looking up a window by class.
                unsafe { FindWindowW(class.as_ptr(), std::ptr::null()) }
            }
        };
        if window.is_null() {
            return Err("the editor's window was not found".into());
        }
        let (width, height) = capture(window, &out)?;
        println!("captured {width}x{height} to {out}");

        gui.destroy(&mut handle);
        if let Some(parent) = parent {
            // SAFETY: the parent this function made.
            unsafe { DestroyWindow(parent) };
        }
        pump_for(Duration::from_millis(100));
        println!("destroyed");
        Ok(())
    }
}
