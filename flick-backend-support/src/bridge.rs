/// Whether an RT build may use the separately installed Effect adapter.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum EffectBridge {
    #[default]
    Absent,
    Available,
    /// Opaque UI values and registries belong to the Effect adapter.
    AvailableWithUi,
    /// Http, RPC and server-function values belong to the Effect adapter.
    AvailableWithWeb,
    /// Both UI and web values belong to the Effect adapter.
    AvailableWithUiAndWeb,
}

impl EffectBridge {
    pub const fn available(self) -> bool {
        !matches!(self, Self::Absent)
    }

    /// The available bridge with the families whose values the adapter owns.
    pub const fn owning(ui: bool, web: bool) -> Self {
        match (ui, web) {
            (false, false) => Self::Available,
            (true, false) => Self::AvailableWithUi,
            (false, true) => Self::AvailableWithWeb,
            (true, true) => Self::AvailableWithUiAndWeb,
        }
    }

    pub const fn ui(self) -> bool {
        matches!(self, Self::AvailableWithUi | Self::AvailableWithUiAndWeb)
    }

    pub const fn web(self) -> bool {
        matches!(self, Self::AvailableWithWeb | Self::AvailableWithUiAndWeb)
    }
}
