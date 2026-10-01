#[derive(Clone, Copy, Default)]
pub enum Mode {
    #[default]
    Insert,
    Navigation,
}

impl Mode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Insert => "INS",
            Self::Navigation => "NAV",
        }
    }
}
