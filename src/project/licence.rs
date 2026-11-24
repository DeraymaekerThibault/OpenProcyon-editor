//! The licence of a new mod: a short list, and the notice written to the mod's `LICENSE` file. The editor ships no licence texts
//! (PROVISIONAL: a notice with the SPDX id, the holder and the URL, not the full text; the modder replaces it before sharing).

/// The licences offered for a new mod.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Licence {
    #[default]
    CcBy4,
    CcBySa4,
    Cc0,
    Gpl3OrLater,
    AllRightsReserved,
}

impl Licence {
    /// Every choice, the default first.
    pub const ALL: [Licence; 5] = [Licence::CcBy4, Licence::CcBySa4, Licence::Cc0, Licence::Gpl3OrLater, Licence::AllRightsReserved];

    /// The SPDX identifier (`LicenseRef-All-Rights-Reserved` is the SPDX way to name "no licence granted").
    pub fn spdx(self) -> &'static str {
        match self {
            Licence::CcBy4 => "CC-BY-4.0",
            Licence::CcBySa4 => "CC-BY-SA-4.0",
            Licence::Cc0 => "CC0-1.0",
            Licence::Gpl3OrLater => "GPL-3.0-or-later",
            Licence::AllRightsReserved => "LicenseRef-All-Rights-Reserved",
        }
    }

    /// The name shown in the list.
    pub fn label(self) -> &'static str {
        match self {
            Licence::AllRightsReserved => "All rights reserved",
            other => other.spdx(),
        }
    }

    /// Where the full text is; `None` when there is none to point to.
    pub fn url(self) -> Option<&'static str> {
        match self {
            Licence::CcBy4 => Some("https://creativecommons.org/licenses/by/4.0/legalcode"),
            Licence::CcBySa4 => Some("https://creativecommons.org/licenses/by-sa/4.0/legalcode"),
            Licence::Cc0 => Some("https://creativecommons.org/publicdomain/zero/1.0/legalcode"),
            Licence::Gpl3OrLater => Some("https://www.gnu.org/licenses/gpl-3.0.txt"),
            Licence::AllRightsReserved => None,
        }
    }

    /// The `LICENSE` file for mod `name` by `author` (empty: the mod's name stands for the holder).
    pub fn notice(self, name: &str, author: &str) -> String {
        let holder = if author.trim().is_empty() { name } else { author.trim() };
        let mut s = format!("{name}\nCopyright (c) {holder}\n\nSPDX-License-Identifier: {}\n", self.spdx());
        match self.url() {
            Some(u) => s.push_str(&format!("\nThis mod is offered under {}. The full text is at:\n{u}\n", self.label())),
            None => s.push_str("\nAll rights reserved: nobody may copy or share this mod without the holder's permission.\n"),
        }
        s.push_str("\nThis file is a notice, not the licence text. Replace it with the full text of the licence before you share the mod.\n");
        s.push_str("The mod holds only your own work and changes to the game's data, never a copy of the game's files.\n");
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_notice_names_the_id_the_holder_and_asks_for_the_full_text() {
        assert_eq!(Licence::default(), Licence::CcBy4);
        for l in Licence::ALL {
            let n = l.notice("My Mod", "Ada");
            assert!(n.contains(&format!("SPDX-License-Identifier: {}", l.spdx())) && n.contains("Copyright (c) Ada") && n.contains("Replace it with the full text"), "{n}");
            assert_eq!(l.url().is_some_and(|u| n.contains(u)), l.url().is_some());
        }
        assert!(Licence::Cc0.notice("M", "  ").contains("Copyright (c) M"), "an empty author falls back to the mod's name");
    }
}
