use crate::{
    data::token::{Token, TokenKey, TOKEN_KEY_LEN},
    server::{Error, Result, Server},
};
use axum::{
    async_trait,
    extract::FromRequestParts,
    http::{request::Parts, HeaderMap},
};
use base64::prelude::{Engine as _, BASE64_STANDARD};
use deadpool_postgres::Pool;
use std::sync::Arc;
use time::OffsetDateTime;
use uuid::Uuid;

/// Authentication middleware.
pub struct Auth {
    pub token: Token,
}

impl Auth {
    /// Parse access token and return token ID and key.
    pub fn parse_access_token(token: &str) -> Option<(Uuid, TokenKey)> {
        let data = BASE64_STANDARD.decode(token).ok()?;

        const UUID_LEN: usize = 16;
        if data.len() != UUID_LEN + TOKEN_KEY_LEN {
            return None;
        }

        Some((
            Uuid::from_slice(&data[..UUID_LEN]).unwrap(),
            data[UUID_LEN..].try_into().unwrap(),
        ))
    }

    /// Compose access token from token ID and secret key.
    pub fn compose_access_token(token: Uuid, key: TokenKey) -> String {
        let mut data = token.as_bytes().to_vec();
        data.extend_from_slice(&key);
        BASE64_STANDARD.encode(data)
    }

    /// Authenticate request and create an Auth instance.
    pub async fn create(pool: &Pool, headers: &HeaderMap) -> Result<Self> {
        use Error::*;
        let Some(authorization) = headers.get("Authorization") else {
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
        let Some((id, key)) = Self::parse_access_token(token) else {
            return Err(Unauthorized(ACCESS_DENIED.to_owned()));
        };

        let client = pool.get().await?;
        let Some(token) = Token::get_and_authenticate(&client, id, key).await? else {
            return Err(Unauthorized(ACCESS_DENIED.to_owned()));
        };

        if token.expires_at < OffsetDateTime::now_utc() {
            return Err(Unauthorized("token expired".to_owned()));
        }

        Ok(Self { token })
    }

    /// Get associated user.
    pub fn user(&self) -> Result<Uuid> {
        self.token.user.ok_or(Error::Unauthorized(
            "token not associated with user".to_owned(),
        ))
    }
}

#[async_trait]
impl FromRequestParts<Arc<Server>> for Auth {
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, server: &Arc<Server>) -> Result<Self> {
        Self::create(&server.pg_pool, &parts.headers).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_parse_token_str() {
        assert_eq!(
            Auth::parse_access_token(
                "QKvO9M1eSniqWjAsQQO9snP2IWWsggdV0l8/jCqgATpOyYUZpuAcOjyt8YJcKjxN"
            ),
            Some((
                Uuid::parse_str("40abcef4-cd5e-4a78-aa5a-302c4103bdb2").unwrap(),
                [
                    0x73, 0xf6, 0x21, 0x65, 0xac, 0x82, 0x07, 0x55, 0xd2, 0x5f, 0x3f, 0x8c, 0x2a,
                    0xa0, 0x01, 0x3a, 0x4e, 0xc9, 0x85, 0x19, 0xa6, 0xe0, 0x1c, 0x3a, 0x3c, 0xad,
                    0xf1, 0x82, 0x5c, 0x2a, 0x3c, 0x4d
                ]
            ))
        )
    }
}
