#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Poller {
    /// Burns 100% CPU. Latency: <10us.
    Busy,
    /// Sleeps immediately. Latency: >50us. Saves Power.
    Wait,
    /// Spins for a short duration, then sleeps. Best general-purpose balance.
    Adaptive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CongestionStrategy {
    /// Return an error immediately if ring is full.
    DropNew,
    /// Block the thread until space is available.
    Block,
}
