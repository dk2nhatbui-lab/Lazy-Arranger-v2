//! Typed, stable identifiers and the marker type.

use serde::{Deserialize, Serialize};

use music_core::Tick;

macro_rules! id_type {
    ($name:ident, $prefix:literal, $doc:literal) => {
        #[doc = $doc]
        ///
        /// Identifiers are allocated by the song and never reused, so an undo
        /// record can name an object that was later removed.
        #[derive(
            Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(try_from = "u32", into = "u32")]
        pub struct $name(u32);

        impl $name {
            /// Creates an identifier from its raw value.
            ///
            /// # Errors
            ///
            /// Returns an error when the value is zero, which never names an
            /// object.
            pub fn new(value: u32) -> Result<Self, IdError> {
                if value == 0 {
                    Err(IdError {
                        kind: $prefix,
                        value,
                    })
                } else {
                    Ok(Self(value))
                }
            }

            /// The raw value.
            #[must_use]
            pub const fn get(self) -> u32 {
                self.0
            }
        }

        impl TryFrom<u32> for $name {
            type Error = IdError;

            fn try_from(value: u32) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl From<$name> for u32 {
            fn from(value: $name) -> Self {
                value.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}{}", $prefix, self.0)
            }
        }
    };
}

/// A rejected identifier value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("{kind} identifier {value} is invalid: identifiers start at 1")]
pub struct IdError {
    /// Short kind name, for example `track-`.
    pub kind: &'static str,
    /// The rejected value.
    pub value: u32,
}

id_type!(TrackId, "track-", "Identifies a track inside one song.");
id_type!(NoteId, "note-", "Identifies a note inside one track.");
id_type!(
    StyleId,
    "style-",
    "Identifies a style referenced by the song."
);
id_type!(ClipId, "clip-", "Identifies a clip inside one track.");

/// A named position on the song timeline.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Marker {
    /// Position in ticks.
    pub tick: Tick,
    /// Display name.
    pub name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_start_at_one() {
        assert!(TrackId::new(0).is_err());
        let id = TrackId::new(7).expect("valid");
        assert_eq!(id.get(), 7);
        assert_eq!(id.to_string(), "track-7");
    }

    #[test]
    fn ids_serialize_as_numbers() {
        let id = NoteId::new(3).expect("valid");
        assert_eq!(serde_json::to_string(&id).expect("serialize"), "3");
        let back: NoteId = serde_json::from_str("3").expect("deserialize");
        assert_eq!(back, id);
        assert!(serde_json::from_str::<NoteId>("0").is_err());
    }
}
