use crate::{
    data::token::Token,
    server::{Error, Result, Server},
};
use axum::{async_trait, extract::FromRequestParts, http::request::Parts};
use base64::prelude::{Engine as _, BASE64_STANDARD};
use std::sync::Arc;
use uuid::Uuid;

/// Authentication middleware.
pub struct Auth {
    pub token: Token,
}

impl Auth {
    /// Length of token key before hashing.
    pub const TOKEN_KEY_LEN: usize = 32;

    /// Parse bearer token and return token ID and key.
    pub fn parse_token_str(token: &str) -> Option<(Uuid, String)> {
        let data = BASE64_STANDARD.decode(token).ok()?;

        const UUID_LEN: usize = 16;
        if data.len() != UUID_LEN + Self::TOKEN_KEY_LEN {
            return None;
        }

        Some((
            Uuid::from_slice(&data[..UUID_LEN]).unwrap(),
            BASE64_STANDARD.encode(&data[UUID_LEN..]),
        ))
    }
}

#[async_trait]
impl FromRequestParts<Arc<Server>> for Auth {
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, server: &Arc<Server>) -> Result<Self> {
        use Error::*;
        let Some(authorization) = parts.headers.get("Authorization") else {
            return Err(Unauthorized("missing Authorization header".to_owned()));
        };

        let Ok(authorization) = authorization.to_str() else {
            return Err(Unauthorized(
                "failed to decode Authorization header".to_owned(),
            ));
        };

        let Some(token) = authorization.strip_prefix("Bearer ") else {
            return Err(Unauthorized("unsupported authorization scheme".to_owned()));
        };

        const ACCESS_DENIED: &str = "access denied";
        let Some((id, key)) = Self::parse_token_str(token) else {
            return Err(Unauthorized(ACCESS_DENIED.to_owned()));
        };

        let client = server.pool.get().await?;
        let Some(token) = Token::authorize(&client, id, &key).await? else {
            return Err(Unauthorized(ACCESS_DENIED.to_owned()));
        };

        Ok(Self { token })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_parse_token_str() {
        assert_eq!(
            Auth::parse_token_str(
                "QKvO9M1eSniqWjAsQQO9snP2IWWsggdV0l8/jCqgATpOyYUZpuAcOjyt8YJcKjxN"
            ),
            Some((
                Uuid::parse_str("40abcef4-cd5e-4a78-aa5a-302c4103bdb2").unwrap(),
                "c/YhZayCB1XSXz+MKqABOk7JhRmm4Bw6PK3xglwqPE0=".to_owned()
            ))
        )
    }
}
