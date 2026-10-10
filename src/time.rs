//! Checked frame views; stored ticks, fractions and source time spaces stay intact.
use crate::{Composition, Keyframe, Layer, Marker};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameRate(f64);
impl FrameRate {
    pub fn new(value: f64) -> Option<Self> {
        (value.is_finite() && value > 0.0 && (1.0 / value).is_finite()).then_some(Self(value))
    }
    pub fn value(self) -> f64 {
        self.0
    }
    pub fn frame_duration(self) -> f64 {
        1.0 / self.0
    }
    /// Round to the nearest frame, with ties away from zero.
    pub fn frames(self, seconds: f64) -> Option<i64> {
        let value = (seconds * self.0).round();
        (value.is_finite() && value >= i64::MIN as f64 && value < i64::MAX as f64)
            .then_some(value as i64)
    }
    pub fn seconds(self, frames: i64) -> Option<f64> {
        let value = frames as f64 / self.0;
        value.is_finite().then_some(value)
    }
}
impl Composition {
    pub fn valid_frame_rate(&self) -> Option<FrameRate> {
        FrameRate::new(self.frame_rate?)
    }
    pub fn display_start_frame(&self) -> Option<i64> {
        self.valid_frame_rate()?
            .frames(self.display_start?.value()?)
    }
    pub fn work_area_frames(&self) -> Option<(i64, i64)> {
        let rate = self.valid_frame_rate()?;
        let start = rate.frames(self.work_area_start?.value()?)?;
        let end = rate.frames(if self.work_area_uses_duration {
            self.duration_seconds?
        } else {
            self.work_area_end?.value()?
        })?;
        (end >= start).then_some((start, end))
    }
}
impl Layer {
    /// Original source-relative in/out fractions converted at an explicit rate.
    /// Does not apply layer start time, stretch or time remapping.
    pub fn source_frame_range(&self, rate: FrameRate) -> Option<(i64, i64)> {
        Some((
            rate.frames(self.source_in_point?.value()?)?,
            rate.frames(self.source_out_point?.value()?)?,
        ))
    }
    pub fn start_frame(&self, rate: FrameRate) -> Option<i64> {
        rate.frames(self.start_time?.value()?)
    }
}
impl Keyframe {
    pub fn frame_time(&self, rate: FrameRate) -> Option<i64> {
        rate.frames(self.time_seconds)
    }
}
impl Marker {
    pub fn frame_time(&self, rate: FrameRate) -> Option<i64> {
        rate.frames(self.time_seconds)
    }
    /// Uses the exposed compatibility duration; its verification flag still applies.
    pub fn frame_duration(&self, rate: FrameRate) -> Option<i64> {
        rate.frames(self.duration_seconds?)
    }
}
