use serde::ser::{Serialize, SerializeStruct, Serializer};

/// Crate error returned by commands. Serializes as `{ kind, message }`.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Tauri(#[from] tauri::Error),
    #[error("macOS reports no main display")]
    NoMainDisplay,
}

impl Error {
    fn kind(&self) -> &'static str {
        match self {
            Self::Tauri(_) => "tauri",
            Self::NoMainDisplay => "no_main_display",
        }
    }
}

impl Serialize for Error {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut state = serializer.serialize_struct("Error", 2)?;
        state.serialize_field("kind", self.kind())?;
        state.serialize_field("message", &self.to_string())?;
        state.end()
    }
}

#[cfg(test)]
mod tests;
