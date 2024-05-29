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

#[derive(Deserialize)]
pub struct PostRequest {
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
    WithRejection(Json(request), _): WithRejection<Json<PostRequest>, Error>,
) -> Result<Response> {
    let mut client = server.pg_pool.get().await?;
    if let Some(token) = Token::find_last_with_ip_address(&client, addr.ip()).await? {
        if token.created_at > OffsetDateTime::now_utc() - Duration::from_secs(3600) {
            return Err(Error::BadRequest("too frequent token requests".to_owned()));
        }
    }

    let user = match Auth::create(&server.pg_pool, &headers).await {
        Ok(auth) => auth.token.user,
        Err(Error::Unauthorized(_)) if request.email.is_some() => None,
        Err(err) => return Err(err),
    };

    let expires_at = request
        .expires_at
        .unwrap_or(OffsetDateTime::new_utc(Date::MAX, Time::MIDNIGHT));

    let mut token = Token::new(
        expires_at,
        request.label,
        user,
        request.is_admin.unwrap_or_default(),
        addr.ip(),
        request.email,
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
