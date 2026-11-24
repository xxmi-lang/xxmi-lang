//! The known profiles. Each ships a library snapshot (built with `xxmi index`), except the
//! generic one.

use serde::Serialize;

/// Which flavour of 3DMigoto ini a profile's MI loads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Dialect {
    Xxmi,
    Gimi,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Profile {
    /// Lower-case: the `--mi-package` value, the snapshot name and the `Core/<MI>` folder.
    pub id: &'static str,
    pub name: &'static str,
    pub dialect: Dialect,
    /// The embedded library snapshot JSON.
    pub snapshot: Option<&'static str>,
}

macro_rules! snapshot {
    ($id:literal) => {
        Some(include_str!(concat!(
            env!("XXMI_SNAPSHOTS_DIR"),
            "/",
            $id,
            ".json"
        )))
    };
}

pub static PROFILES: [Profile; 7] = [
    Profile {
        id: "xxmi",
        name: "XXMI (generic)",
        dialect: Dialect::Xxmi,
        snapshot: None,
    },
    Profile {
        id: "gimi",
        name: "GIMI (Genshin Impact)",
        dialect: Dialect::Gimi,
        snapshot: snapshot!("gimi"),
    },
    Profile {
        id: "srmi",
        name: "SRMI (Honkai: Star Rail)",
        dialect: Dialect::Xxmi,
        snapshot: snapshot!("srmi"),
    },
    Profile {
        id: "wwmi",
        name: "WWMI (Wuthering Waves)",
        dialect: Dialect::Xxmi,
        snapshot: snapshot!("wwmi"),
    },
    Profile {
        id: "zzmi",
        name: "ZZMI (Zenless Zone Zero)",
        dialect: Dialect::Xxmi,
        snapshot: snapshot!("zzmi"),
    },
    Profile {
        id: "himi",
        name: "HIMI (Honkai Impact 3rd)",
        dialect: Dialect::Xxmi,
        snapshot: snapshot!("himi"),
    },
    Profile {
        id: "efmi",
        name: "EFMI (Arknights: Endfield)",
        dialect: Dialect::Xxmi,
        snapshot: snapshot!("efmi"),
    },
];

/// The profile of a mod that names no MI: the XXMI dialect and no library.
pub fn generic() -> &'static Profile {
    &PROFILES[0]
}

/// Case-insensitive.
pub fn by_id(id: &str) -> Option<&'static Profile> {
    PROFILES.iter().find(|p| p.id.eq_ignore_ascii_case(id))
}

pub fn ids() -> impl Iterator<Item = &'static str> {
    PROFILES.iter().map(|p| p.id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::snapshot::SymbolSnapshot;

    #[test]
    fn every_snapshot_parses_and_is_named_after_its_profile() {
        for profile in &PROFILES {
            let Some(json) = profile.snapshot else {
                continue;
            };
            let snapshot: SymbolSnapshot = serde_json::from_str(json).unwrap();
            assert_eq!(snapshot.name, profile.id);
        }
    }

    #[test]
    fn only_gimi_has_its_own_dialect() {
        for profile in &PROFILES {
            assert_eq!(profile.dialect == Dialect::Gimi, profile.id == "gimi");
        }
    }

    #[test]
    fn ids_are_found_in_any_case() {
        assert_eq!(by_id("WWMI").map(|p| p.id), Some("wwmi"));
        assert!(by_id("nope").is_none());
        assert!(generic().snapshot.is_none());
    }
}
