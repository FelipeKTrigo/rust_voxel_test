#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlayerMode {
    Survival,
    #[default]
    Creative,
    Spectator,
}

impl PlayerMode {
    /// Base horizontal speed in blocks/s.
    pub fn speed(&self) -> f32 {
        match self {
            PlayerMode::Survival => 4.3,
            PlayerMode::Creative => 5.6,
            PlayerMode::Spectator => 10.9,
        }
    }

    pub fn can_fly(&self) -> bool {
        matches!(self, PlayerMode::Creative | PlayerMode::Spectator)
    }

    pub fn noclip(&self) -> bool {
        matches!(self, PlayerMode::Spectator)
    }

    /// Whether blocks can be broken freely (Creative: instantly).
    pub fn can_break_blocks(&self) -> bool {
        matches!(self, PlayerMode::Creative)
    }

    /// Whether the player can interact with the world at all.
    pub fn can_interact(&self) -> bool {
        !matches!(self, PlayerMode::Spectator)
    }

    pub fn label(&self) -> &'static str {
        match self {
            PlayerMode::Survival => "Survival",
            PlayerMode::Creative => "Creative",
            PlayerMode::Spectator => "Spectator",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_properties() {
        assert_eq!(PlayerMode::default(), PlayerMode::Creative);
        assert!(!PlayerMode::Survival.can_fly());
        assert!(PlayerMode::Creative.can_fly() && !PlayerMode::Creative.noclip());
        assert!(PlayerMode::Spectator.noclip());
        assert!(PlayerMode::Creative.can_break_blocks());
        assert!(!PlayerMode::Survival.can_break_blocks());
        assert!(!PlayerMode::Spectator.can_break_blocks());
        assert_eq!(PlayerMode::Survival.label(), "Survival");
    }
}
