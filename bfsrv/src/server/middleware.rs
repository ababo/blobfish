use crate::server::Server;
use axum::{
    async_trait,
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
};
use std::{result::Result as StdResult, sync::Arc};
use uuid::Uuid;

/// Authentication middleware.
pub struct Auth {
    pub user: Uuid,
}

#[async_trait]
impl FromRequestParts<Arc<Server>> for Auth {
    type Rejection = StatusCode;

    async fn from_request_parts(
        _parts: &mut Parts,
        _server: &Arc<Server>,
    ) -> StdResult<Self, Self::Rejection> {
        Ok(Self { user: Uuid::nil() })
    }
}
