use crate::{PanteonError, FEATURE_REPORT_LEN};

/// Captured hardware state snapshot consisting of ordered SET_REPORT frames and intervals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateSnapshot {
    pub source_pid: u16,
    pub captured_at: String,
    pub label: String,
    pub frames: Vec<[u8; FEATURE_REPORT_LEN]>,
    pub delays_ms: Vec<u16>,
}

impl StateSnapshot {
    pub fn new(
        source_pid: u16,
        captured_at: impl Into<String>,
        label: impl Into<String>,
        frames: Vec<[u8; FEATURE_REPORT_LEN]>,
        delays_ms: Vec<u16>,
    ) -> Result<Self, PanteonError> {
        let snapshot = Self {
            source_pid,
            captured_at: captured_at.into(),
            label: label.into(),
            frames,
            delays_ms,
        };
        snapshot.validate()?;
        Ok(snapshot)
    }

    pub fn validate(&self) -> Result<(), PanteonError> {
        if self.frames.len() != self.delays_ms.len() {
            return Err(PanteonError::ProtocolDetailed {
                context: "StateSnapshot::validate",
                detail: format!(
                    "frames count ({}) does not match delays_ms count ({})",
                    self.frames.len(),
                    self.delays_ms.len()
                ),
            });
        }
        Ok(())
    }
}
