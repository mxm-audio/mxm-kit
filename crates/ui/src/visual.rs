//! Shared geometry for software-native telemetry views.
//!
//! These values are visual-system decisions, not properties of any one instrument. A plugin owns
//! the data and semantic labels; this module owns the canvas radius, trace hierarchy and plot
//! rhythm so telemetry does not quietly create a second design system.

use crate::space::{RADIUS, SPACE_1, SPACE_2};

pub const PLOT_HEIGHT: f32 = 84.0;
pub const TALL_PLOT_HEIGHT: f32 = 96.0;
pub const STATUS_HEIGHT: f32 = 60.0;
pub const CANVAS_RADIUS: f32 = RADIUS as f32;
pub const AXIS_STROKE: f32 = 1.0;
pub const REFERENCE_STROKE: f32 = 1.0;
pub const TRACE_STROKE: f32 = 1.5;
pub const EMPHASIS_STROKE: f32 = 2.0;
pub const METER_STROKE: f32 = 3.0;
pub const LIVE_MARKER_RADIUS: f32 = SPACE_1;
pub const INNER_GUTTER: f32 = SPACE_2;
pub const METER_LABEL_WIDTH: f32 = 92.0;
pub const TRACE_REACH: f32 = 0.42;
pub const SPLIT_TRACE_REACH: f32 = 0.20;
pub const UPPER_TRACE_CENTRE: f32 = 0.28;
pub const LOWER_TRACE_CENTRE: f32 = 0.72;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn telemetry_geometry_uses_the_shared_grid_and_a_visible_stroke_hierarchy() {
        for extent in [PLOT_HEIGHT, TALL_PLOT_HEIGHT, STATUS_HEIGHT] {
            assert_eq!(extent % SPACE_1, 0.0);
        }
        assert_eq!(CANVAS_RADIUS, RADIUS as f32);
        let hierarchy = [
            AXIS_STROKE,
            REFERENCE_STROKE,
            TRACE_STROKE,
            EMPHASIS_STROKE,
            METER_STROKE,
        ];
        assert!(hierarchy.windows(2).all(|pair| pair[0] <= pair[1]));
        let centres = [UPPER_TRACE_CENTRE, LOWER_TRACE_CENTRE];
        assert_eq!(centres.iter().sum::<f32>(), 1.0);
    }
}
