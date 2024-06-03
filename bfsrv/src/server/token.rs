use crate::{
    data::token::Token,
    server::{middleware::Auth, Error, Result, Server},
};
use axum::{
    extract::{ConnectInfo, State},
    http::HeaderMap,
    response::{IntoResponse, Response},
    Json,
};
use axum_extra::extract::WithRejection;
use lettre::Address as EmailAddress;
use serde::Deserialize;
use serde_json::{json, Map};
use std::{net::SocketAddr, sync::Arc, time::Duration};
use time::{Date, OffsetDateTime, Time};

/// Body payload for PATCH-request.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostRequestPayload {
    expires_at: Option<OffsetDateTime>,
    label: Option<String>,
    is_admin: Option<bool>,
    email: Option<EmailAddress>,
}

/// Handle token POST requests.
pub async fn handle_token_post(
    State(server): State<Arc<Server>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    WithRejection(Json(payload), _): WithRejection<Json<PostRequestPayload>, Error>,
) -> Result<Response> {
    let mut client = server.pg_pool.get().await?;
    if let Some(token) = Token::find_last_with_ip_address(&client, addr.ip()).await? {
        if token.created_at > OffsetDateTime::now_utc() - Duration::from_secs(3600) {
            return Err(Error::BadRequest("too frequent token requests".to_owned()));
        }
    }

    let user = match Auth::create(&server.pg_pool, &headers).await {
        Ok(auth) => auth.token.user,
        Err(Error::Unauthorized(_)) if payload.email.is_some() => None,
        Err(err) => return Err(err),
    };

    let never = OffsetDateTime::new_utc(Date::MAX, Time::MIDNIGHT);
    let expires_at = payload.expires_at.unwrap_or(never);

    let mut token = Token::new(
        expires_at,
        payload.label,
        user,
        payload.is_admin.unwrap_or_default(),
        addr.ip(),
        payload.email,
    );

    let tx = client.build_transaction().start().await?;

    let key = token.insert(&tx).await?;
    let access_token = Auth::compose_access_token(token.id, key);

    let mut response = Map::new();
    if let Some(email) = token.email {
        server.mailer.send_token(email, &access_token).await?;
    } else {
        response["id"] = json!(token.id);
        response["token"] = json!(access_token);
    }

    tx.commit().await?;

    Ok(Json(response).into_response())
}
