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
use time::{format_description::well_known::Rfc3339, Date, OffsetDateTime, Time};

/// Body payload for POST-request.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostRequestPayload {
    promo_code: Option<String>,
}

/// Handle user GET requests.
pub async fn handle_user_get(State(server): State<Arc<Server>>, auth: Auth) -> Result<Response> {
    let user_id = auth.user()?;

    use Error::*;
    let client = server.pg_pool.get().await?;
    let Some(user) = User::get(&client, user_id).await? else {
        return Err(Internal("user not found".to_owned()));
    };

    let mut json = json!({
        "user": {
            "id": user.id,
            "createdAt": user.created_at.format(&Rfc3339).unwrap(),
            "email": user.email,
            "campaign": user.campaign,
            "balance": user.balance,
        }
    });
    if let Some(referrer) = user.referrer {
        json["referrer"] = json!(referrer);
    }

    Ok(Json(json).into_response())
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
