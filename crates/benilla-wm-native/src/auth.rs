use std::fmt;

use crate::error::{Error, Result};

const MIN_TOKEN_BYTES: usize = 16;
const MAX_TOKEN_BYTES: usize = 512;

/// An owner-provided local bearer token.
///
/// The token must be 16..=512 printable ASCII bytes. It is intentionally not `Display` and its
/// `Debug` output is redacted. The caller is responsible for sourcing it from an owner-controlled
/// secret and must never embed it in source or logs.
#[derive(Clone, PartialEq, Eq)]
pub struct AuthToken(String);

impl AuthToken {
    pub fn new(token: impl Into<String>) -> Result<Self> {
        let token = token.into();
        let len = token.len();
        if !(MIN_TOKEN_BYTES..=MAX_TOKEN_BYTES).contains(&len) {
            return Err(Error::Config(
                "token must be between 16 and 512 printable ASCII bytes",
            ));
        }
        if !token.bytes().all(|byte| (33..=126).contains(&byte)) {
            return Err(Error::Config("token must contain printable ASCII only"));
        }
        Ok(Self(token))
    }

    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for AuthToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AuthToken(<redacted>)")
    }
}
