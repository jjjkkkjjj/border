use super::{Record, Recorder};

/// A recorder that ignores any record. This struct is used just for debugging.
pub struct NullRecorder {}

impl NullRecorder {}

impl Recorder for NullRecorder {
    /// Discard the given record.
    fn write(&mut self, _record: Record) {}

    fn store(&mut self, _record: Record) {}

    fn flush(&mut self, _step: i64) {}
}
