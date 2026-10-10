//! Button glyph names that follow the device in use: the same prompt reads "A" on a pad and
//! "ENTER" on a keyboard. Pure text; `widgets::glyph` draws the chip.

use super::focus::UiKey;

/// The device whose labels a screen shows: whichever was touched last.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Device {
    Pad,
    #[default]
    Keys,
}

/// What a prompt names.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Glyph {
    Confirm,
    Back,
    Tabs,
    Move,
    Adjust,
    Page,
    Alt1,
    Alt2,
}

impl Glyph {
    pub fn label(self, device: Device) -> &'static str {
        match (self, device) {
            (Self::Confirm, Device::Pad) => "A",
            (Self::Confirm, Device::Keys) => "ENTER",
            (Self::Back, Device::Pad) => "B",
            (Self::Back, Device::Keys) => "ESC",
            (Self::Tabs, Device::Pad) => "LB RB",
            (Self::Tabs, Device::Keys) => "[ ]",
            (Self::Move, Device::Pad) => "D-PAD",
            (Self::Move, Device::Keys) => "UP DOWN",
            (Self::Adjust, Device::Pad) => "LEFT RIGHT",
            (Self::Adjust, Device::Keys) => "LEFT RIGHT",
            (Self::Page, Device::Pad) => "LT RT",
            (Self::Page, Device::Keys) => "PGUP PGDN",
            (Self::Alt1, Device::Pad) => "X",
            (Self::Alt1, Device::Keys) => "DEL",
            (Self::Alt2, Device::Pad) => "Y",
            (Self::Alt2, Device::Keys) => "INS",
        }
    }

    /// The logical key a glyph stands for, where it is one key.
    pub fn key(self) -> Option<UiKey> {
        match self {
            Self::Confirm => Some(UiKey::Confirm),
            Self::Back => Some(UiKey::Back),
            Self::Alt1 => Some(UiKey::Alt1),
            Self::Alt2 => Some(UiKey::Alt2),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_glyph_has_a_label_per_device() {
        for glyph in [
            Glyph::Confirm,
            Glyph::Back,
            Glyph::Tabs,
            Glyph::Move,
            Glyph::Adjust,
            Glyph::Page,
            Glyph::Alt1,
            Glyph::Alt2,
        ] {
            assert!(!glyph.label(Device::Pad).is_empty());
            assert!(!glyph.label(Device::Keys).is_empty());
        }
        assert_eq!(Glyph::Confirm.label(Device::Pad), "A");
        assert_eq!(Glyph::Confirm.label(Device::Keys), "ENTER");
    }
}
