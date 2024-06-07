use crate::{
    data::{
        payment::{Payment, PaymentProcessor, PaymentStatus},
        user::User,
    },
    server::{middleware::Auth, Error, Result, Server},
};
use axum::{
    extract::{Json, State},
    response::{IntoResponse, Response},
};
use axum_extra::extract::WithRejection;
use deadpool_postgres::Client;
use log::info;
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::json;
use std::{sync::Arc, time::Duration};
use time::OffsetDateTime;
use tokio::time::interval;
use tokio_postgres::{error::SqlState, IsolationLevel};
use uuid::Uuid;

/// Body payload for PATCH-request.
#[derive(Deserialize)]
pub struct PatchRequestPayload {
    reference: String,
}

/// Handle payment PATCH requests.
pub async fn handle_payment_patch(
    State(server): State<Arc<Server>>,
    WithRejection(Json(payload), _): WithRejection<Json<PatchRequestPayload>, Error>,
) -> Result<Response> {
    let mut client = server.pg_pool.get().await?;

    let mut interval = interval(Duration::from_millis(10));
    let mut remains = 100;

    loop {
        interval.tick().await;

        let result =
            try_update_payment_status_atomically(server.as_ref(), &payload, &mut client).await;
        if !is_serialization_failure(&result) {
            break result;
        }

        remains -= 1;
        if remains == 0 {
            break result;
        }
    }
}

async fn try_update_payment_status_atomically(
    server: &Server,
    payload: &PatchRequestPayload,
    client: &mut Client,
) -> Result<Response> {
    let tx = client
        .build_transaction()
        .isolation_level(IsolationLevel::RepeatableRead)
        .start()
        .await?;

    use Error::*;
    let Some(mut payment) = Payment::get_by_reference(&tx, &payload.reference).await? else {
        return Err(PaymentNotFound);
    };

    use PaymentStatus::*;
    if !matches!(payment.status, New) {
        return Ok(Json(json!({ "status": payment.status })).into_response());
    }

    let (status, details) = match payment.processor {
        PaymentProcessor::Paypal => server.paypal.retrieve_status(&payment.reference).await?,
    };

    if !matches!(status, Completed) {
        payment.status = status;
        payment.details = details;
        payment.update(&tx).await?;
        tx.commit().await?;
        return Ok(Json(json!({ "status": payment.status })).into_response());
    }

    let Some(amount) = server
        .currency_converter
        .convert(&payment.currency, payment.amount)
        .await?
    else {
        return Err(Internal(format!(
            "failed to get currency rate for payment {}",
            payment.id
        )));
    };

    let mut payment = Payment::get_by_reference(&tx, &payload.reference)
        .await?
        .unwrap();
    payment.status = status;
    payment.details = details;
    payment.update(&tx).await?;

    let Some(mut user) = User::get(&tx, payment.to_user).await? else {
        return Err(Internal(format!(
            "failed to get to_user for payment {}",
            payment.id
        )));
    };
    user.balance += amount;
    user.update(&tx).await?;

    tx.commit().await?;

    info!("completed payment {}", payment.id);
    Ok(Json(json!({ "status": payment.status })).into_response())
}

#[inline]
fn is_serialization_failure<T>(error: &Result<T>) -> bool {
    const SQL_STATE: Option<&SqlState> = Some(&SqlState::T_R_SERIALIZATION_FAILURE);
    use Error::*;
    matches!(error, Err(Data(crate::data::Error::Postgres(e))) if e.code() == SQL_STATE)
        || matches!(error, Err(Postgres(e)) if e.code() == SQL_STATE)
}

/// Body payload for POST-request.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostRequestPayload {
    currency: String,
    amount: Decimal,
    processor: PaymentProcessor,
    to_user: Option<Uuid>,
    locale: Option<String>,
}

/// Handle payment POST requests.
pub async fn handle_payment_post(
    State(server): State<Arc<Server>>,
    auth: Auth,
    WithRejection(Json(payload), _): WithRejection<Json<PostRequestPayload>, Error>,
) -> Result<Response> {
    let user = auth.user()?;

    let client = server.pg_pool.get().await?;
    if let Some(payment) = Payment::find_last_with_from_user(&client, user).await? {
        if payment.created_at > OffsetDateTime::now_utc() - Duration::from_secs(3600) {
            return Err(Error::BadRequest(
                "too frequent payment requests".to_owned(),
            ));
        }
    }

    use PaymentProcessor::*;
    let (reference, url) = match payload.processor {
        Paypal => {
            server
                .paypal
                .initiate(&payload.currency, payload.amount, payload.locale.as_deref())
                .await?
        }
    };

    let mut payment = Payment::new(
        payload.currency,
        payload.amount,
        user,
        payload.to_user.unwrap_or(user),
        payload.processor,
        reference.clone(),
    );
    payment.insert(&client).await?;

    info!(
        "initiated payment {} of {} {}",
        payment.id, payment.amount, payment.currency
    );
    Ok(Json(json!({ "reference": reference, "url": url })).into_response())
}
