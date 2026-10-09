//! `Linux::IRunLoop`: a VST3 editor's file descriptors and timers, run on the host's main loop.
//!
//! On Linux a VST3 view has no event loop of its own: it asks the host, through the frame, to
//! call it when a file descriptor is readable (its X11 connection, usually) and on timers. Here
//! each goes to CLAP's `posix-fd` or `timer` extension when the host offers it, and is otherwise
//! polled on the main thread, which then asks for the next callback straight away. The floating
//! window's own connection is watched the same way.

use crate::gui::PlugFrame;
use clack_extensions::posix_fd::{FdFlags, HostPosixFd};
use clack_extensions::timer::{HostTimer, TimerId};
use clack_plugin::host::{HostMainThreadHandle, HostSharedHandle};
use std::cell::RefCell;
use std::os::fd::RawFd;
use std::time::{Duration, Instant};
use vst3::Steinberg::Linux::{
    FileDescriptor, IEventHandler, IEventHandlerTrait, IRunLoopTrait, ITimerHandler,
    ITimerHandlerTrait, TimerInterval,
};
use vst3::Steinberg::{kInvalidArgument, kResultOk, tresult};
use vst3::{ComPtr, ComRef};

#[derive(Clone)]
enum Watcher {
    Plugin(ComPtr<IEventHandler>),
    Window(crate::window::Handle),
}

struct Fd {
    fd: RawFd,
    watcher: Watcher,
    /// Whether the host watches it; otherwise it is polled.
    registered: bool,
}

struct Timer {
    handler: ComPtr<ITimerHandler>,
    interval: Duration,
    /// The host's timer, when it runs one; otherwise it is polled.
    id: Option<TimerId>,
    due: Instant,
}

pub(crate) struct RunLoop {
    host: HostSharedHandle<'static>,
    fds: RefCell<Vec<Fd>>,
    timers: RefCell<Vec<Timer>>,
    /// Registrations to withdraw from the host at the next service.
    gone_fds: RefCell<Vec<RawFd>>,
    gone_timers: RefCell<Vec<TimerId>>,
}

impl RunLoop {
    pub(crate) fn new(host: HostSharedHandle<'static>) -> Self {
        Self {
            host,
            fds: RefCell::new(Vec::new()),
            timers: RefCell::new(Vec::new()),
            gone_fds: RefCell::new(Vec::new()),
            gone_timers: RefCell::new(Vec::new()),
        }
    }

    /// Reads the floating window's events whenever its connection has some.
    pub(crate) fn watch_window(&self, window: &crate::window::Window) {
        let handle = window.handle();
        self.fds.borrow_mut().push(Fd {
            fd: handle.fd(),
            watcher: Watcher::Window(handle),
            registered: false,
        });
    }

    /// Brings the host's registrations in line, and polls whatever it does not watch.
    pub(crate) fn service(&self, host: &mut HostMainThreadHandle) {
        let posix = host.shared().get_extension::<HostPosixFd>();
        let timers = host.shared().get_extension::<HostTimer>();
        for fd in self.gone_fds.take() {
            if let Some(posix) = &posix {
                let _ = posix.unregister_fd(host, fd);
            }
        }
        for id in self.gone_timers.take() {
            if let Some(timers) = &timers {
                let _ = timers.unregister_timer(host, id);
            }
        }
        if let Some(posix) = &posix {
            for fd in self.fds.borrow_mut().iter_mut().filter(|fd| !fd.registered) {
                fd.registered = posix
                    .register_fd(host, fd.fd, FdFlags::READ | FdFlags::ERROR)
                    .is_ok();
            }
        }
        if let Some(timer_support) = &timers {
            for timer in self
                .timers
                .borrow_mut()
                .iter_mut()
                .filter(|t| t.id.is_none())
            {
                let period = timer.interval.as_millis().clamp(1, u128::from(u32::MAX)) as u32;
                timer.id = timer_support.register_timer(host, period).ok();
            }
        }

        // What the host does not run, runs here.
        for fd in self.ready_unregistered() {
            self.on_fd_raw(fd);
        }
        let now = Instant::now();
        let due: Vec<ComPtr<ITimerHandler>> = self
            .timers
            .borrow_mut()
            .iter_mut()
            .filter(|timer| timer.id.is_none() && timer.due <= now)
            .map(|timer| {
                timer.due = now + timer.interval;
                timer.handler.clone()
            })
            .collect();
        for handler in due {
            // SAFETY: on the main thread; the handler is the plugin's, kept alive here.
            unsafe { handler.onTimer() };
        }
        let polling = self.fds.borrow().iter().any(|fd| !fd.registered)
            || self.timers.borrow().iter().any(|timer| timer.id.is_none());
        if polling {
            self.host.request_callback();
        }
    }

    /// The unwatched descriptors that are readable now.
    fn ready_unregistered(&self) -> Vec<RawFd> {
        let mut polled: Vec<libc::pollfd> = self
            .fds
            .borrow()
            .iter()
            .filter(|fd| !fd.registered)
            .map(|fd| libc::pollfd {
                fd: fd.fd,
                events: libc::POLLIN,
                revents: 0,
            })
            .collect();
        if polled.is_empty() {
            return Vec::new();
        }
        // SAFETY: `polled` is a valid array of its length; a zero timeout never blocks.
        let ready = unsafe { libc::poll(polled.as_mut_ptr(), polled.len() as libc::nfds_t, 0) };
        if ready <= 0 {
            return Vec::new();
        }
        polled
            .iter()
            .filter(|fd| fd.revents != 0)
            .map(|fd| fd.fd)
            .collect()
    }

    pub(crate) fn on_timer(&self, id: TimerId) {
        let handler = self
            .timers
            .borrow()
            .iter()
            .find(|timer| timer.id == Some(id))
            .map(|timer| timer.handler.clone());
        if let Some(handler) = handler {
            // SAFETY: on the main thread; the handler is the plugin's, kept alive here.
            unsafe { handler.onTimer() };
        }
    }

    pub(crate) fn on_fd(&self, fd: RawFd, _flags: FdFlags) {
        self.on_fd_raw(fd);
    }

    fn on_fd_raw(&self, fd: RawFd) {
        // Taken out first: a handler may register or unregister while it runs.
        let watchers: Vec<Watcher> = self
            .fds
            .borrow()
            .iter()
            .filter(|watched| watched.fd == fd)
            .map(|watched| watched.watcher.clone())
            .collect();
        for watcher in watchers {
            match watcher {
                // SAFETY: on the main thread; the handler is the plugin's, kept alive here.
                Watcher::Plugin(handler) => unsafe { handler.onFDIsSet(fd) },
                Watcher::Window(window) => window.pump(),
            }
        }
    }

    /// Forgets everything, and withdraws it from the host: the editor is closing.
    pub(crate) fn clear(&self, host: &mut HostMainThreadHandle) {
        for fd in self.fds.take() {
            if fd.registered {
                self.gone_fds.borrow_mut().push(fd.fd);
            }
        }
        for timer in self.timers.take() {
            if let Some(id) = timer.id {
                self.gone_timers.borrow_mut().push(id);
            }
        }
        self.service(host);
    }

    /// Stops watching the floating window, which is about to go.
    pub(crate) fn forget_window(&self) {
        self.forget(|fd| !matches!(fd.watcher, Watcher::Window(_)));
    }

    fn forget(&self, keep: impl Fn(&Fd) -> bool) {
        let mut fds = self.fds.borrow_mut();
        let mut gone = self.gone_fds.borrow_mut();
        fds.retain(|fd| {
            let kept = keep(fd);
            if !kept && fd.registered {
                gone.push(fd.fd);
            }
            kept
        });
    }
}

impl IRunLoopTrait for PlugFrame {
    unsafe fn registerEventHandler(
        &self,
        handler: *mut IEventHandler,
        fd: FileDescriptor,
    ) -> tresult {
        // SAFETY: the plugin passes a live handler; the reference added here is ours.
        let Some(handler) = (unsafe { ComRef::from_raw(handler) }).map(|h| h.to_com_ptr()) else {
            return kInvalidArgument;
        };
        let run_loop = &self.state.run_loop;
        run_loop.fds.borrow_mut().push(Fd {
            fd,
            watcher: Watcher::Plugin(handler),
            registered: false,
        });
        run_loop.host.request_callback();
        kResultOk
    }

    unsafe fn unregisterEventHandler(&self, handler: *mut IEventHandler) -> tresult {
        let run_loop = &self.state.run_loop;
        run_loop
            .forget(|fd| !matches!(&fd.watcher, Watcher::Plugin(held) if held.as_ptr() == handler));
        run_loop.host.request_callback();
        kResultOk
    }

    unsafe fn registerTimer(
        &self,
        handler: *mut ITimerHandler,
        milliseconds: TimerInterval,
    ) -> tresult {
        // SAFETY: the plugin passes a live handler; the reference added here is ours.
        let Some(handler) = (unsafe { ComRef::from_raw(handler) }).map(|h| h.to_com_ptr()) else {
            return kInvalidArgument;
        };
        let run_loop = &self.state.run_loop;
        let interval = Duration::from_millis(milliseconds.max(1));
        run_loop.timers.borrow_mut().push(Timer {
            handler,
            interval,
            id: None,
            due: Instant::now() + interval,
        });
        run_loop.host.request_callback();
        kResultOk
    }

    unsafe fn unregisterTimer(&self, handler: *mut ITimerHandler) -> tresult {
        let run_loop = &self.state.run_loop;
        let mut gone = run_loop.gone_timers.borrow_mut();
        run_loop.timers.borrow_mut().retain(|timer| {
            let kept = timer.handler.as_ptr() != handler;
            if !kept && let Some(id) = timer.id {
                gone.push(id);
            }
            kept
        });
        kResultOk
    }
}
