/// Trait for converting various timestamp representations into unix epoch seconds (u64).
pub trait IntoTimestamp {
    fn into_timestamp(self) -> u64;
}

impl IntoTimestamp for u64 {
    fn into_timestamp(self) -> u64 {
        self
    }
}

impl IntoTimestamp for &str {
    fn into_timestamp(self) -> u64 {
        self.parse::<u64>().unwrap_or(0)
    }
}

impl IntoTimestamp for String {
    fn into_timestamp(self) -> u64 {
        self.parse::<u64>().unwrap_or(0)
    }
}

impl IntoTimestamp for &String {
    fn into_timestamp(self) -> u64 {
        self.parse::<u64>().unwrap_or(0)
    }
}

/// Shadow ledger entry representing a recorded hardware write
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShadowEntry {
    pub bank: u8,
    pub addr: u16,          // хранить как u16, сериализовать десятичным
    pub data: u16,
    pub written_at: u64,    // unix-секунды из std::time (SystemTime -> seconds)
}

impl ShadowEntry {
    pub fn new(bank: u8, addr: u16, data: u16, written_at: impl IntoTimestamp) -> Self {
        Self {
            bank,
            addr,
            data,
            written_at: written_at.into_timestamp(),
        }
    }
}

/// Shadow ledger containing device hint and list of recorded writes
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ShadowLedger {
    pub device_hint: String, // "/dev/hidrawN" или "unknown" — при переподключении предупреждает
    pub entries: Vec<ShadowEntry>,
}

impl ShadowLedger {
    pub fn new(device_hint: impl Into<String>) -> Self {
        Self {
            device_hint: device_hint.into(),
            entries: Vec::new(),
        }
    }

    pub fn append(&mut self, entry: ShadowEntry) {
        self.entries.push(entry);
    }
}
