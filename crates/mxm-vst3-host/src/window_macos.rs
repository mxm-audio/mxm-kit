//! A floating editor's window on macOS: an `NSWindow`, whose content view the plugin's view is
//! attached to, with a delegate that reports resizing and turns the close button into hiding.

use super::{EventHandler, WindowEvent};
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{
    NSBackingStoreType, NSView, NSWindow, NSWindowDelegate, NSWindowOrderingMode, NSWindowStyleMask,
};
use objc2_foundation::{
    NSNotification, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString,
};
use std::cell::{Cell, RefCell};
use std::ffi::c_void;

struct DelegateIvars {
    on_event: RefCell<EventHandler>,
    /// The window it is the delegate of; it lives as long as the delegate is set.
    window: Cell<*const NSWindow>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "MxmVst3EditorWindowDelegate"]
    #[ivars = DelegateIvars]
    struct Delegate;

    unsafe impl NSObjectProtocol for Delegate {}

    unsafe impl NSWindowDelegate for Delegate {
        #[unsafe(method(windowShouldClose:))]
        fn window_should_close(&self, _sender: &NSWindow) -> bool {
            self.send(WindowEvent::CloseRequested);
            false
        }

        #[unsafe(method(windowDidResize:))]
        fn window_did_resize(&self, _notification: &NSNotification) {
            let window = self.ivars().window.get();
            // SAFETY: the delegate is unset before the window goes.
            if let Some(window) = unsafe { window.as_ref() }
                && let Some(view) = window.contentView()
            {
                let size = view.frame().size;
                self.send(WindowEvent::Resized(size.width as u32, size.height as u32));
            }
        }
    }
);

impl Delegate {
    fn new(marker: MainThreadMarker, on_event: EventHandler) -> Retained<Self> {
        let this = Self::alloc(marker).set_ivars(DelegateIvars {
            on_event: RefCell::new(on_event),
            window: Cell::new(std::ptr::null()),
        });
        // SAFETY: `NSObject`'s designated initialiser.
        unsafe { msg_send![super(this), init] }
    }

    fn send(&self, event: WindowEvent) {
        // An event the handler itself causes arrives while it runs: it is the handler's own.
        if let Ok(mut on_event) = self.ivars().on_event.try_borrow_mut() {
            on_event(event);
        }
    }
}

/// What resizing and showing need: the window.
#[derive(Clone, Copy)]
pub(crate) struct Handle(*const NSWindow);

impl Handle {
    fn window(&self) -> &NSWindow {
        // SAFETY: a handle is only kept while its window lives (`gui.rs` clears it first).
        unsafe { &*self.0 }
    }

    pub(crate) fn resize(&self, width: u32, height: u32) {
        self.window()
            .setContentSize(NSSize::new(f64::from(width), f64::from(height)));
    }

    pub(crate) fn set_visible(&self, visible: bool) {
        if visible {
            self.window().makeKeyAndOrderFront(None);
        } else {
            self.window().orderOut(None);
        }
    }
}

pub(crate) struct Window {
    window: Retained<NSWindow>,
    view: Retained<NSView>,
    delegate: Retained<Delegate>,
}

impl Window {
    pub(crate) fn new(
        title: &str,
        width: u32,
        height: u32,
        resizable: bool,
        on_event: EventHandler,
    ) -> Result<Self, String> {
        let marker = MainThreadMarker::new().ok_or("not on the main thread")?;
        let mut style = NSWindowStyleMask::Titled
            | NSWindowStyleMask::Closable
            | NSWindowStyleMask::Miniaturizable;
        if resizable {
            style |= NSWindowStyleMask::Resizable;
        }
        let frame = NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(f64::from(width), f64::from(height)),
        );
        // SAFETY: a plain window, kept alive by this struct rather than released on close.
        let window = unsafe {
            let window = NSWindow::initWithContentRect_styleMask_backing_defer(
                NSWindow::alloc(marker),
                frame,
                style,
                NSBackingStoreType::Buffered,
                false,
            );
            window.setReleasedWhenClosed(false);
            window
        };
        window.setTitle(&NSString::from_str(title));
        window.center();
        let delegate = Delegate::new(marker, on_event);
        delegate.ivars().window.set(Retained::as_ptr(&window));
        window.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
        let view = window
            .contentView()
            .ok_or("the window has no content view")?;
        Ok(Self {
            window,
            view,
            delegate,
        })
    }

    pub(crate) fn native(&self) -> *mut c_void {
        Retained::as_ptr(&self.view) as *mut c_void
    }

    pub(crate) fn handle(&self) -> Handle {
        Handle(Retained::as_ptr(&self.window))
    }

    pub(crate) fn set_owner(&self, owner: *mut c_void) {
        // SAFETY: CLAP's cocoa window is the host's `NSView`.
        let Some(owner) = (unsafe { (owner as *const NSView).as_ref() }) else {
            return;
        };
        if let Some(parent) = owner.window() {
            // SAFETY: both windows are live; the child goes with its parent.
            unsafe { parent.addChildWindow_ordered(&self.window, NSWindowOrderingMode::Above) };
        }
    }

    pub(crate) fn set_title(&self, title: &str) {
        self.window.setTitle(&NSString::from_str(title));
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        self.window.setDelegate(None);
        self.delegate.ivars().window.set(std::ptr::null());
        self.window.orderOut(None);
        self.window.close();
    }
}
