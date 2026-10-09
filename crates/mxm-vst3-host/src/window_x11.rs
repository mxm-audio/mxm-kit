//! A floating editor's window on Linux: an X11 window on a connection of its own, which the
//! plugin's view embeds into (`X11EmbedWindowID`). Its events are read on the main thread
//! whenever the run loop finds its connection readable (`run_loop.rs`).

use super::{EventHandler, WindowEvent};
use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::os::fd::{AsRawFd, RawFd};
use x11rb::connection::Connection;
use x11rb::properties::WmSizeHints;
use x11rb::protocol::Event;
use x11rb::protocol::xproto::{
    AtomEnum, ConfigureWindowAux, ConnectionExt as _, CreateWindowAux, EventMask, PropMode,
    WindowClass,
};
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

struct Inner {
    connection: RustConnection,
    window: u32,
    wm_delete: u32,
    resizable: bool,
    size: Cell<(u32, u32)>,
    on_event: RefCell<EventHandler>,
}

impl Inner {
    fn send(&self, event: WindowEvent) {
        // An event the handler itself causes arrives while it runs: it is the handler's own.
        if let Ok(mut on_event) = self.on_event.try_borrow_mut() {
            on_event(event);
        }
    }

    /// Fixes the size a window manager may give it, for an editor that does not resize.
    fn fix_size(&self, width: u32, height: u32) {
        if self.resizable {
            return;
        }
        let size = (width as i32, height as i32);
        let hints = WmSizeHints {
            min_size: Some(size),
            max_size: Some(size),
            ..WmSizeHints::default()
        };
        let _ = hints.set_normal_hints(&self.connection, self.window);
    }
}

/// What resizing, showing and reading events need: the window.
#[derive(Clone, Copy)]
pub(crate) struct Handle(*const Inner);

impl Handle {
    fn inner(&self) -> &Inner {
        // SAFETY: a handle is only kept while its window lives (`gui.rs` clears it first).
        unsafe { &*self.0 }
    }

    pub(crate) fn resize(&self, width: u32, height: u32) {
        let inner = self.inner();
        inner.fix_size(width, height);
        let _ = inner.connection.configure_window(
            inner.window,
            &ConfigureWindowAux::new().width(width).height(height),
        );
        let _ = inner.connection.flush();
        inner.size.set((width, height));
    }

    pub(crate) fn set_visible(&self, visible: bool) {
        let inner = self.inner();
        let _ = if visible {
            inner.connection.map_window(inner.window).map(drop)
        } else {
            inner.connection.unmap_window(inner.window).map(drop)
        };
        let _ = inner.connection.flush();
    }

    /// Handles every event waiting on the window's connection.
    pub(crate) fn pump(&self) {
        let inner = self.inner();
        while let Ok(Some(event)) = inner.connection.poll_for_event() {
            match event {
                Event::ConfigureNotify(event) if event.window == inner.window => {
                    let size = (u32::from(event.width), u32::from(event.height));
                    if size != inner.size.get() {
                        inner.size.set(size);
                        inner.send(WindowEvent::Resized(size.0, size.1));
                    }
                }
                Event::ClientMessage(event)
                    if event.window == inner.window
                        && event.data.as_data32()[0] == inner.wm_delete =>
                {
                    inner.send(WindowEvent::CloseRequested);
                }
                _ => {}
            }
        }
    }

    /// The connection's file descriptor, readable when events wait.
    pub(crate) fn fd(&self) -> RawFd {
        self.inner().connection.stream().as_raw_fd()
    }
}

pub(crate) struct Window {
    inner: Box<Inner>,
}

impl Window {
    pub(crate) fn new(
        title: &str,
        width: u32,
        height: u32,
        resizable: bool,
        on_event: EventHandler,
    ) -> Result<Self, String> {
        let text = |error: &dyn std::fmt::Display| error.to_string();
        let (connection, screen) = x11rb::connect(None).map_err(|e| text(&e))?;
        let root = connection.setup().roots[screen].root;
        let black = connection.setup().roots[screen].black_pixel;
        let window = connection.generate_id().map_err(|e| text(&e))?;
        connection
            .create_window(
                x11rb::COPY_DEPTH_FROM_PARENT,
                window,
                root,
                0,
                0,
                width.min(u16::MAX.into()) as u16,
                height.min(u16::MAX.into()) as u16,
                0,
                WindowClass::INPUT_OUTPUT,
                x11rb::COPY_FROM_PARENT,
                &CreateWindowAux::new()
                    .background_pixel(black)
                    .event_mask(EventMask::STRUCTURE_NOTIFY),
            )
            .map_err(|e| text(&e))?;
        let atom = |name: &[u8]| -> Result<u32, String> {
            Ok(connection
                .intern_atom(false, name)
                .map_err(|e| text(&e))?
                .reply()
                .map_err(|e| text(&e))?
                .atom)
        };
        let wm_protocols = atom(b"WM_PROTOCOLS")?;
        let wm_delete = atom(b"WM_DELETE_WINDOW")?;
        connection
            .change_property32(
                PropMode::REPLACE,
                window,
                wm_protocols,
                AtomEnum::ATOM,
                &[wm_delete],
            )
            .map_err(|e| text(&e))?;
        let inner = Box::new(Inner {
            connection,
            window,
            wm_delete,
            resizable,
            size: Cell::new((width, height)),
            on_event: RefCell::new(on_event),
        });
        inner.fix_size(width, height);
        let this = Self { inner };
        this.set_title(title);
        let _ = this.inner.connection.flush();
        Ok(this)
    }

    pub(crate) fn native(&self) -> *mut c_void {
        self.inner.window as usize as *mut c_void
    }

    pub(crate) fn handle(&self) -> Handle {
        Handle(&*self.inner)
    }

    pub(crate) fn set_owner(&self, owner: *mut c_void) {
        let owner = owner as usize as u32;
        let _ = self.inner.connection.change_property32(
            PropMode::REPLACE,
            self.inner.window,
            AtomEnum::WM_TRANSIENT_FOR,
            AtomEnum::WINDOW,
            &[owner],
        );
        let _ = self.inner.connection.flush();
    }

    pub(crate) fn set_title(&self, title: &str) {
        let connection = &self.inner.connection;
        let _ = connection.change_property8(
            PropMode::REPLACE,
            self.inner.window,
            AtomEnum::WM_NAME,
            AtomEnum::STRING,
            title.as_bytes(),
        );
        let names = connection
            .intern_atom(false, b"_NET_WM_NAME")
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .zip(
                connection
                    .intern_atom(false, b"UTF8_STRING")
                    .ok()
                    .and_then(|cookie| cookie.reply().ok()),
            );
        if let Some((name, utf8)) = names {
            let _ = connection.change_property8(
                PropMode::REPLACE,
                self.inner.window,
                name.atom,
                utf8.atom,
                title.as_bytes(),
            );
        }
        let _ = connection.flush();
    }

    /// Handles every event waiting on the window's connection.
    pub(crate) fn pump(&self) {
        self.handle().pump();
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        let _ = self.inner.connection.destroy_window(self.inner.window);
        let _ = self.inner.connection.flush();
    }
}
