//! The plugin's editor: `IPlugView` behind CLAP's `gui` extension.
//!
//! **Embedded**, the host gives a window and the view is attached to it (`set_parent`), as every
//! VST3 host does. **Floating**, which CLAP allows and VST3 does not know, the view is attached to
//! a top-level window of this crate's (`window.rs`), which the host can make transient for its own.
//! Either way the view asks to change size through `IPlugFrame::resizeView`: embedded, that is
//! CLAP's `request_resize`; floating, this crate's window resizes.
//!
//! On Linux a view also needs the host's event loop (`Linux::IRunLoop`): its file descriptors and
//! timers run through CLAP's `posix-fd` and `timer` extensions when the host has them, and are
//! polled on the main thread otherwise (`run_loop.rs`).

use crate::plugin::MainThread;
use crate::window::{Window, WindowEvent};
use clack_extensions::gui::{
    GuiApiType, GuiConfiguration, GuiResizeHints, GuiSize, HostGui, PluginGuiImpl,
    Window as ClapWindow,
};
use clack_extensions::timer::{PluginTimerImpl, TimerId};
use clack_plugin::host::HostSharedHandle;
use clack_plugin::prelude::*;
use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::rc::Rc;
use vst3::Steinberg::Vst::{IEditControllerTrait, ViewType};
use vst3::Steinberg::{
    FIDString, IPlugFrame, IPlugFrameTrait, IPlugView, IPlugViewContentScaleSupport,
    IPlugViewContentScaleSupportTrait, IPlugViewTrait, ViewRect, kResultFalse, kResultOk,
    kResultTrue, tresult,
};
use vst3::{Class, ComPtr, ComWrapper};

#[cfg(target_os = "windows")]
const PLATFORM_TYPE: FIDString = vst3::Steinberg::kPlatformTypeHWND;
#[cfg(target_os = "macos")]
const PLATFORM_TYPE: FIDString = vst3::Steinberg::kPlatformTypeNSView;
#[cfg(target_os = "linux")]
const PLATFORM_TYPE: FIDString = vst3::Steinberg::kPlatformTypeX11EmbedWindowID;

#[cfg(target_os = "windows")]
const PLATFORM_API: GuiApiType<'static> = GuiApiType::WIN32;
#[cfg(target_os = "macos")]
const PLATFORM_API: GuiApiType<'static> = GuiApiType::COCOA;
#[cfg(target_os = "linux")]
const PLATFORM_API: GuiApiType<'static> = GuiApiType::X11;

fn rect(size: GuiSize) -> ViewRect {
    ViewRect {
        left: 0,
        top: 0,
        right: size.width as i32,
        bottom: size.height as i32,
    }
}

fn size_of(rect: &ViewRect) -> GuiSize {
    GuiSize {
        width: (rect.right - rect.left).max(0) as u32,
        height: (rect.bottom - rect.top).max(0) as u32,
    }
}

/// What the frame, the floating window and the plugin share, on the main thread only.
pub(crate) struct FrameState {
    /// Valid while the editor is: the view is told to forget the frame before the editor goes.
    host: HostSharedHandle<'static>,
    host_gui: Option<HostGui>,
    /// The floating window, if this editor floats.
    window: RefCell<Option<crate::window::Handle>>,
    /// Set while this side resizes, so the window's own resize event is not sent back.
    resizing: Cell<bool>,
    /// The floating window's close button was pressed; the host is told on the main thread.
    closed: Cell<bool>,
    #[cfg(target_os = "linux")]
    pub(crate) run_loop: crate::run_loop::RunLoop,
}

/// `IPlugFrame` (and on Linux `IRunLoop`): what the view talks to.
pub(crate) struct PlugFrame {
    pub(crate) state: Rc<FrameState>,
}

#[cfg(not(target_os = "linux"))]
impl Class for PlugFrame {
    type Interfaces = (IPlugFrame,);
}

#[cfg(target_os = "linux")]
impl Class for PlugFrame {
    type Interfaces = (IPlugFrame, vst3::Steinberg::Linux::IRunLoop);
}

impl IPlugFrameTrait for PlugFrame {
    unsafe fn resizeView(&self, view: *mut IPlugView, new_size: *mut ViewRect) -> tresult {
        if view.is_null() || new_size.is_null() {
            return kResultFalse;
        }
        // SAFETY: the view passes a valid rectangle.
        let size = size_of(unsafe { &*new_size });
        let floating = *self.state.window.borrow();
        match floating {
            Some(window) => {
                self.state.resizing.set(true);
                window.resize(size.width, size.height);
                self.state.resizing.set(false);
            }
            None => {
                let accepted = self.state.host_gui.as_ref().is_some_and(|gui| {
                    gui.request_resize(&self.state.host, size.width, size.height)
                        .is_ok()
                });
                if !accepted {
                    return kResultFalse;
                }
            }
        }
        // The host's half of the protocol: the view hears its new size from the host.
        // SAFETY: `view` is the live view that called, and `new_size` its rectangle.
        if let Some(view) = unsafe { vst3::ComRef::from_raw(view) } {
            unsafe { view.onSize(new_size) };
        }
        kResultTrue
    }
}

/// An open editor.
pub(crate) struct Editor {
    view: ComPtr<IPlugView>,
    _frame: ComWrapper<PlugFrame>,
    state: Rc<FrameState>,
    window: Option<Window>,
    attached: bool,
}

impl Editor {
    fn native_parent(window: &ClapWindow) -> Option<*mut c_void> {
        #[cfg(target_os = "windows")]
        return window.as_win32_hwnd();
        #[cfg(target_os = "macos")]
        return window.as_cocoa_nsview();
        #[cfg(target_os = "linux")]
        return window
            .as_x11_handle()
            .map(|handle| handle as usize as *mut c_void);
    }

    fn attach(&mut self, parent: *mut c_void) -> Result<(), PluginError> {
        // SAFETY: on the main thread; `parent` is a live native window of the platform's type.
        if unsafe { self.view.attached(parent, PLATFORM_TYPE) } != kResultOk {
            return Err(PluginError::Message("the VST3 editor did not attach"));
        }
        self.attached = true;
        Ok(())
    }
}

impl Drop for Editor {
    fn drop(&mut self) {
        // SAFETY: on the main thread; the reverse of opening.
        unsafe {
            if self.attached {
                self.view.removed();
            }
            self.view.setFrame(std::ptr::null_mut());
        }
        self.state.window.borrow_mut().take();
        #[cfg(target_os = "linux")]
        self.state.run_loop.forget_window();
        self.window = None;
    }
}

/// The view's reaction to its floating window being resized or closed by the user.
fn window_event(view: &ComPtr<IPlugView>, state: &FrameState, event: WindowEvent) {
    match event {
        WindowEvent::Resized(width, height) => {
            if state.resizing.get() {
                return;
            }
            let mut wanted = rect(GuiSize { width, height });
            // SAFETY: on the main thread, the view live.
            unsafe {
                if view.canResize() != kResultTrue {
                    return;
                }
                view.checkSizeConstraint(&mut wanted);
                let allowed = size_of(&wanted);
                if (allowed.width, allowed.height) != (width, height)
                    && let Some(window) = *state.window.borrow()
                {
                    state.resizing.set(true);
                    window.resize(allowed.width, allowed.height);
                    state.resizing.set(false);
                }
                view.onSize(&mut wanted);
            }
        }
        WindowEvent::CloseRequested => {
            if let Some(window) = *state.window.borrow() {
                window.set_visible(false);
            }
            state.closed.set(true);
            state.host.request_callback();
        }
    }
}

impl MainThread<'_> {
    /// Tells the host about a floating editor its user closed, and on Linux runs the view's part
    /// of the event loop when the host does not.
    pub(crate) fn service_editor(&mut self) {
        let Some(editor) = &self.editor else {
            return;
        };
        if editor.state.closed.replace(false)
            && let Some(gui) = editor.state.host_gui.as_ref()
        {
            gui.closed(&editor.state.host, false);
        }
        #[cfg(target_os = "linux")]
        if let Some(editor) = &self.editor {
            if let Some(window) = &editor.window {
                window.pump();
            }
            editor.state.run_loop.service(&mut self.host);
        }
    }
}

impl PluginGuiImpl for MainThread<'_> {
    fn is_api_supported(&mut self, configuration: GuiConfiguration) -> bool {
        configuration.api_type == PLATFORM_API && self.shared.vst.controller.is_some()
    }

    fn get_preferred_api(&mut self) -> Option<GuiConfiguration<'_>> {
        Some(GuiConfiguration {
            api_type: PLATFORM_API,
            is_floating: false,
        })
    }

    fn create(&mut self, configuration: GuiConfiguration) -> Result<(), PluginError> {
        if self.editor.is_some() || configuration.api_type != PLATFORM_API {
            return Err(PluginError::Message("unsupported editor configuration"));
        }
        let controller = self
            .shared
            .vst
            .controller
            .as_ref()
            .ok_or(PluginError::Message("the VST3 plugin has no controller"))?;
        // SAFETY: on the main thread. `createView` hands over one reference.
        let view = unsafe { ComPtr::from_raw(controller.createView(ViewType::kEditor)) }
            .ok_or(PluginError::Message("the VST3 plugin has no editor"))?;
        // SAFETY: on the main thread.
        if unsafe { view.isPlatformTypeSupported(PLATFORM_TYPE) } != kResultTrue {
            return Err(PluginError::Message(
                "the VST3 editor cannot show on this platform",
            ));
        }

        // SAFETY: the view is told to forget the frame (`Editor::drop`) before the plugin, and with
        // it the host handle, goes.
        let host: HostSharedHandle<'static> =
            unsafe { self.host.shared().with_arbitrary_lifetime() };
        let state = Rc::new(FrameState {
            host,
            host_gui: host.get_extension::<HostGui>(),
            window: RefCell::new(None),
            resizing: Cell::new(false),
            closed: Cell::new(false),
            #[cfg(target_os = "linux")]
            run_loop: crate::run_loop::RunLoop::new(host),
        });
        let frame = ComWrapper::new(PlugFrame {
            state: state.clone(),
        });
        let frame_ptr = frame
            .as_com_ref::<IPlugFrame>()
            .map_or(std::ptr::null_mut(), |frame| frame.as_ptr());
        // SAFETY: on the main thread; the frame outlives the view's use of it.
        unsafe { view.setFrame(frame_ptr) };

        let mut editor = Editor {
            view: view.clone(),
            _frame: frame,
            state: state.clone(),
            window: None,
            attached: false,
        };
        if configuration.is_floating {
            let mut size = ViewRect {
                left: 0,
                top: 0,
                right: 640,
                bottom: 480,
            };
            // SAFETY: on the main thread.
            let resizable = unsafe {
                view.getSize(&mut size);
                view.canResize() == kResultTrue
            };
            let size = size_of(&size);
            let event_view = view.clone();
            let event_state = state.clone();
            let window = Window::new(
                &self.shared.name,
                size.width.max(1),
                size.height.max(1),
                resizable,
                Box::new(move |event| window_event(&event_view, &event_state, event)),
            )
            .map_err(|_| PluginError::Message("no window for the VST3 editor"))?;
            *state.window.borrow_mut() = Some(window.handle());
            let parent = window.native();
            editor.window = Some(window);
            editor.attach(parent)?;
        }
        self.editor = Some(editor);
        #[cfg(target_os = "linux")]
        if let Some(editor) = &self.editor {
            if let Some(window) = &editor.window {
                editor.state.run_loop.watch_window(window);
            }
            editor.state.run_loop.service(&mut self.host);
        }
        Ok(())
    }

    fn destroy(&mut self) {
        #[cfg(target_os = "linux")]
        if let Some(editor) = &self.editor {
            editor.state.run_loop.clear(&mut self.host);
        }
        self.editor = None;
    }

    fn set_scale(&mut self, scale: f64) -> Result<(), PluginError> {
        let editor = self
            .editor
            .as_ref()
            .ok_or(PluginError::Message("no editor"))?;
        let support = editor
            .view
            .cast::<IPlugViewContentScaleSupport>()
            .ok_or(PluginError::Message("the VST3 editor does not scale"))?;
        // SAFETY: on the main thread.
        if unsafe { support.setContentScaleFactor(scale as f32) } == kResultOk {
            Ok(())
        } else {
            Err(PluginError::Message("the VST3 editor refused the scale"))
        }
    }

    fn get_size(&mut self) -> Option<GuiSize> {
        let editor = self.editor.as_ref()?;
        let mut size = ViewRect {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        // SAFETY: on the main thread.
        (unsafe { editor.view.getSize(&mut size) } == kResultOk).then(|| size_of(&size))
    }

    fn can_resize(&mut self) -> bool {
        self.editor
            .as_ref()
            // SAFETY: on the main thread.
            .is_some_and(|editor| unsafe { editor.view.canResize() } == kResultTrue)
    }

    fn get_resize_hints(&mut self) -> Option<GuiResizeHints> {
        None
    }

    fn adjust_size(&mut self, size: GuiSize) -> Option<GuiSize> {
        let editor = self.editor.as_ref()?;
        let mut wanted = rect(size);
        // SAFETY: on the main thread.
        unsafe { editor.view.checkSizeConstraint(&mut wanted) };
        Some(size_of(&wanted))
    }

    fn set_size(&mut self, size: GuiSize) -> Result<(), PluginError> {
        let editor = self
            .editor
            .as_ref()
            .ok_or(PluginError::Message("no editor"))?;
        if let Some(window) = &editor.window {
            editor.state.resizing.set(true);
            window.handle().resize(size.width, size.height);
            editor.state.resizing.set(false);
        }
        let mut wanted = rect(size);
        // SAFETY: on the main thread.
        if unsafe { editor.view.onSize(&mut wanted) } == kResultOk {
            Ok(())
        } else {
            Err(PluginError::Message("the VST3 editor refused the size"))
        }
    }

    fn set_parent(&mut self, window: ClapWindow) -> Result<(), PluginError> {
        let editor = self
            .editor
            .as_mut()
            .ok_or(PluginError::Message("no editor"))?;
        if editor.window.is_some() {
            return Err(PluginError::Message("a floating editor has no parent"));
        }
        let parent = Editor::native_parent(&window)
            .ok_or(PluginError::Message("a parent of another platform"))?;
        editor.attach(parent)?;
        #[cfg(target_os = "linux")]
        editor.state.run_loop.service(&mut self.host);
        Ok(())
    }

    fn set_transient(&mut self, window: ClapWindow) -> Result<(), PluginError> {
        let editor = self
            .editor
            .as_ref()
            .ok_or(PluginError::Message("no editor"))?;
        let floating = editor
            .window
            .as_ref()
            .ok_or(PluginError::Message("an embedded editor is not transient"))?;
        let owner = Editor::native_parent(&window)
            .ok_or(PluginError::Message("a window of another platform"))?;
        floating.set_owner(owner);
        Ok(())
    }

    fn suggest_title(&mut self, title: &str) {
        if let Some(window) = self
            .editor
            .as_ref()
            .and_then(|editor| editor.window.as_ref())
        {
            window.set_title(title);
        }
    }

    fn show(&mut self) -> Result<(), PluginError> {
        let editor = self
            .editor
            .as_ref()
            .ok_or(PluginError::Message("no editor"))?;
        if let Some(window) = &editor.window {
            window.handle().set_visible(true);
        }
        Ok(())
    }

    fn hide(&mut self) -> Result<(), PluginError> {
        let editor = self
            .editor
            .as_ref()
            .ok_or(PluginError::Message("no editor"))?;
        if let Some(window) = &editor.window {
            window.handle().set_visible(false);
        }
        Ok(())
    }
}

impl PluginTimerImpl for MainThread<'_> {
    fn on_timer(&mut self, timer_id: TimerId) {
        #[cfg(target_os = "linux")]
        if let Some(editor) = &self.editor {
            editor.state.run_loop.on_timer(timer_id);
        }
        #[cfg(not(target_os = "linux"))]
        let _ = timer_id;
    }
}

#[cfg(unix)]
impl clack_extensions::posix_fd::PluginPosixFdImpl for MainThread<'_> {
    fn on_fd(&mut self, fd: std::os::fd::RawFd, flags: clack_extensions::posix_fd::FdFlags) {
        #[cfg(target_os = "linux")]
        if let Some(editor) = &self.editor {
            editor.state.run_loop.on_fd(fd, flags);
        }
        #[cfg(not(target_os = "linux"))]
        let _ = (fd, flags);
    }
}
