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
use log::info;
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::json;
use std::{sync::Arc, time::Duration};
use time::OffsetDateTime;
use tokio_postgres::IsolationLevel;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct PatchRequest {
    reference: String,
}

/// Handle payment PATCH requests.
pub async fn handle_payment_patch(
    State(server): State<Arc<Server>>,
    WithRejection(request, _): WithRejection<Json<PatchRequest>, Error>,
) -> Result<Response> {
    use Error::*;
    let mut client = server.pool.get().await?;
    let Some(mut payment) = Payment::get_by_reference(&client, &request.reference).await? else {
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
        payment.update(&client).await?;
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

    let tx = client
        .build_transaction()
        .isolation_level(IsolationLevel::RepeatableRead)
        .start()
        .await?;

    let mut payment = Payment::get_by_reference(&tx, &request.reference)
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

#[derive(Deserialize)]
pub struct PostRequest {
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
    WithRejection(Json(request), _): WithRejection<Json<PostRequest>, Error>,
) -> Result<Response> {
    let user = auth.user()?;

    let client = server.pool.get().await?;
    if let Some(created_at) = Payment::find_latest_created_at(&client, user).await? {
        if created_at > OffsetDateTime::now_utc() - Duration::from_secs(3600) {
            return Err(Error::BadRequest("too frequent payments".to_owned()));
        }
    }

    use PaymentProcessor::*;
    let (reference, url) = match request.processor {
        Paypal => {
            server
                .paypal
                .initiate(&request.currency, request.amount, request.locale.as_deref())
                .await?
        }
    };

    let mut payment = Payment::new(
        request.currency,
        request.amount,
        user,
        request.to_user.unwrap_or(user),
        request.processor,
        reference.clone(),
    );
    payment.insert(&client).await?;

    info!(
        "initiated payment {} of {} {}",
        payment.id, payment.amount, payment.currency
    );
    Ok(Json(json!({ "reference": reference, "url": url })).into_response())
}
