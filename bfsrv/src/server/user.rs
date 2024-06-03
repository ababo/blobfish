use crate::{
    data::{campaign::Campaign, token::Token, user::User},
    server::{middleware::Auth, Error, Result, Server},
};
use axum::{
    extract::State,
    response::{IntoResponse, Response},
    Json,
};
use axum_extra::extract::WithRejection;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use time::{Date, OffsetDateTime, Time};

/// Body payload for POST-request.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostRequestPayload {
    promo_code: Option<String>,
}

/// Handle user POST requests.
pub async fn handle_user_post(
    State(server): State<Arc<Server>>,
    mut auth: Auth,
    WithRejection(Json(payload), _): WithRejection<Json<PostRequestPayload>, Error>,
) -> Result<Response> {
    use Error::*;
    let Some(email) = &auth.token.email else {
        return Err(Unauthorized("no email confirmed".to_owned()));
    };

    let mut client = server.pg_pool.get().await?;
    if User::get_by_email(&client, email).await?.is_some() {
        return Err(EmailAlreadyRegistered);
    }

    let promo_code = payload.promo_code.as_deref().unwrap_or("default");
    let Some(campaign) = Campaign::find_by_promo_code(&client, promo_code).await? else {
        return Err(CampaignNotFound);
    };

    let tx = client.build_transaction().start().await?;

    auth.token.expires_at = OffsetDateTime::now_utc();
    auth.token.update(&tx).await?;

    let mut user = User::new(
        email.clone(),
        auth.token.user,
        campaign.id,
        campaign.initial_balance,
    );
    user.insert(&tx).await?;

    let never = OffsetDateTime::new_utc(Date::MAX, Time::MIDNIGHT);
    let mut token = Token::new(
        never,
        Some("admin".to_owned()),
        Some(user.id),
        true,
        auth.token.ip_address,
        None,
    );
    let key = token.insert(&tx).await?;

    tx.commit().await?;

    let access_token = Auth::compose_access_token(token.id, key);
    Ok(Json(json!({ "id": user.id, "tokenId": token.id, "token": access_token })).into_response())
}
