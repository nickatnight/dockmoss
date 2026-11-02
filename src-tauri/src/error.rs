use serde::ser::{Serialize, SerializeStruct, Serializer};

/// Crate error returned by commands. Serializes as `{ kind, message }`.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Tauri(#[from] tauri::Error),
}

impl Error {
    fn kind(&self) -> &'static str {
        match self {
            Self::Tauri(_) => "tauri",
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
mod tests {
    use super::Error;

    #[test]
    fn serializes_as_kind_and_message() {
        let error = Error::Tauri(tauri::Error::WindowNotFound);

        let json = serde_json::to_value(&error).expect("Error serializes to JSON");

        assert_eq!(
            json,
            serde_json::json!({ "kind": "tauri", "message": "window not found" })
        );
    }
}
