//! BACK during a mouse drag cancels it, as it cancels a keyboard edit: the drag ends, and the
//! control that was dragged puts back the value it began at instead of keeping the one it reached
//! (the owner, 2026-10-08: the plugins match newDAWn).
//!
//! egui ends a drag itself when `Escape` is pressed, at the start of the frame and before anything
//! else hears of the key, and the keyboard language then reads the same key as BACK. So the cursor
//! calls [`notice_escape`] before it reads its keys, where the keymap puts BACK on `Escape`, and
//! [`cancel`] when BACK is on another key. Either way the dragged control learns of it from
//! [`cancelled`] in the frame its drag stops.

use egui::{Context, Id, Key};

fn cancelled_id() -> Id {
    Id::new("mxm-drag-cancelled")
}

/// Before the keys are read: if a drag stopped in a frame `Escape` was pressed, marks it cancelled
/// and returns true, so the BACK that follows does nothing else.
pub(crate) fn notice_escape(ctx: &Context) -> bool {
    let Some(stopped) = ctx.drag_stopped_id() else {
        return false;
    };
    if !ctx.input(|input| input.key_pressed(Key::Escape)) {
        return false;
    }
    ctx.data_mut(|data| data.insert_temp(cancelled_id(), stopped));
    true
}

/// Cancels the mouse drag going on, if there is one, and returns whether there was: then BACK did
/// that and nothing else.
pub(crate) fn cancel(ctx: &Context) -> bool {
    let Some(dragged) = ctx.dragged_id() else {
        return false;
    };
    ctx.data_mut(|data| data.insert_temp(cancelled_id(), dragged));
    ctx.stop_dragging();
    true
}

/// Whether this widget's drag, which has just stopped, was cancelled: then it keeps nothing and
/// puts back the value the drag began at. Asked once; the answer is taken.
///
/// Public so an editor's own dragged widget can match the shared controls.
pub fn cancelled(ctx: &Context, widget: Id) -> bool {
    ctx.data_mut(|data| {
        let was = data.get_temp::<Id>(cancelled_id()) == Some(widget);
        if was {
            data.remove::<Id>(cancelled_id());
        }
        was
    })
}
