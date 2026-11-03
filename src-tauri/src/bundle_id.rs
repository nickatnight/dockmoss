use serde::Serialize;

const MAX_BYTES: usize = 255;

/// A macOS bundle identifier such as `com.apple.Safari`: 1 to 255 bytes.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct BundleId(String);

impl BundleId {
    pub fn new(id: impl Into<String>) -> Option<Self> {
        let id = id.into();
        (!id.is_empty() && id.len() <= MAX_BYTES).then_some(Self(id))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests;
