use std::fmt::{self, Display, Formatter};

#[derive(Debug)]
pub struct PatchError {
    message: String,
}

impl PatchError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl Display for PatchError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for PatchError {}

impl From<std::io::Error> for PatchError {
    fn from(error: std::io::Error) -> Self {
        Self::new(error.to_string())
    }
}
