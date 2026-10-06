//! Blocking native work — a file dialog above all — run off the editor frame and collected on a
//! later one.
//!
//! **An editor frame must never wait on a modal OS loop.** A native file dialog pumps the plugin
//! window's messages while it is open. egui-baseview is still inside that window's frame, so the
//! re-entered event handler finds its state already borrowed and panics
//! (`RefCell already mutably borrowed`) inside a window procedure, which cannot unwind: the host
//! process aborts. Clicking Browse took the whole player down this way.
//!
//! [`start`] runs the work on its own thread and requests a repaint when it finishes; [`take`] hands
//! the result to an ordinary frame, which acts on it exactly as it would on a dropped file. The
//! result lives in the egui context's temporary memory under the caller's id, so an editor that
//! closes before the dialog does simply never collects it.

use egui::{Context, Id};

#[derive(Clone)]
enum Slot<T> {
    Running,
    Done(Option<T>),
}

/// Starts `work` on its own thread unless work under `id` is still running, and reports whether it
/// started. A `None` result — a cancelled dialog — is collected as nothing.
pub fn start<T>(ctx: &Context, id: Id, work: impl FnOnce() -> Option<T> + Send + 'static) -> bool
where
    T: Clone + Send + Sync + 'static,
{
    let claimed = ctx.data_mut(|data| {
        if matches!(data.get_temp::<Slot<T>>(id), Some(Slot::Running)) {
            return false;
        }
        data.insert_temp(id, Slot::<T>::Running);
        true
    });
    if !claimed {
        return false;
    }
    let context = ctx.clone();
    let spawned = std::thread::Builder::new()
        .name("mxm-ui-offthread".to_owned())
        .spawn(move || {
            let result = work();
            context.data_mut(|data| data.insert_temp(id, Slot::Done(result)));
            context.request_repaint();
        });
    if spawned.is_err() {
        ctx.data_mut(|data| data.remove::<Slot<T>>(id));
        return false;
    }
    true
}

/// Whether work under `id` has started and not yet finished. A caller disables its trigger on this,
/// so a second click cannot stack a second dialog.
pub fn running<T>(ctx: &Context, id: Id) -> bool
where
    T: Clone + Send + Sync + 'static,
{
    ctx.data(|data| matches!(data.get_temp::<Slot<T>>(id), Some(Slot::Running)))
}

/// The finished result, exactly once. `None` while the work runs, when none was started, and when
/// the work produced nothing.
pub fn take<T>(ctx: &Context, id: Id) -> Option<T>
where
    T: Clone + Send + Sync + 'static,
{
    ctx.data_mut(|data| match data.get_temp::<Slot<T>>(id) {
        Some(Slot::Done(result)) => {
            data.remove::<Slot<T>>(id);
            result
        }
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    fn collect<T: Clone + Send + Sync + 'static>(ctx: &Context, id: Id) -> Option<T> {
        let deadline = Instant::now() + Duration::from_secs(5);
        while running::<T>(ctx, id) {
            assert!(Instant::now() < deadline, "off-thread work never finished");
            std::thread::sleep(Duration::from_millis(1));
        }
        take(ctx, id)
    }

    #[test]
    fn the_frame_returns_while_the_work_blocks_and_collects_the_result_once() {
        let ctx = Context::default();
        let id = Id::new("offthread-result");
        let (release, gate) = mpsc::channel::<()>();
        assert!(start(&ctx, id, move || {
            gate.recv().unwrap();
            Some(7u32)
        }));
        // The caller is back while the work is still blocked, which is the whole point.
        assert!(running::<u32>(&ctx, id));
        assert_eq!(take::<u32>(&ctx, id), None);
        assert!(
            !start(&ctx, id, || Some(8u32)),
            "a second start must not stack work"
        );
        release.send(()).unwrap();
        assert_eq!(collect::<u32>(&ctx, id), Some(7));
        assert_eq!(take::<u32>(&ctx, id), None, "a result is collected once");
        assert!(!running::<u32>(&ctx, id));
    }

    #[test]
    fn a_cancelled_result_is_nothing_and_frees_the_slot() {
        let ctx = Context::default();
        let id = Id::new("offthread-cancel");
        assert!(start(&ctx, id, || None::<String>));
        assert_eq!(collect::<String>(&ctx, id), None);
        assert!(start(&ctx, id, || Some("again".to_owned())));
        assert_eq!(collect::<String>(&ctx, id).as_deref(), Some("again"));
    }
}
