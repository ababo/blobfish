use crate::{
    data::{
        payment::{Payment, PaymentProcessor, PaymentStatus},
        user::User,
    },
    server::{middleware::Auth, Error, Result, Server},
};
use axum::{
    extract::{Json, Query, State},
    response::{IntoResponse, Response},
};
use axum_extra::extract::WithRejection;
use deadpool_postgres::Client;
use log::info;
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::json;
use std::{sync::Arc, time::Duration};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use tokio::time::interval;
use tokio_postgres::{error::SqlState, IsolationLevel};
use uuid::Uuid;

/// Payment GET request query.
#[derive(Deserialize)]
pub struct PaymentQuery {
    id: Option<Uuid>,
}

pub async fn handle_payment_get(
    State(server): State<Arc<Server>>,
    auth: Auth,
    WithRejection(Query(query), _): WithRejection<Query<PaymentQuery>, Error>,
) -> Result<Response> {
    let user = auth.user()?;
    let client = server.pg_pool.get().await?;

    let payments = if let Some(id) = query.id {
        use Error::*;
        let Some(payment) = Payment::get(&client, id).await? else {
            return Err(PaymentNotFound);
        };
        if payment.from_user != user {
            return Err(PaymentNotFound);
        }
        vec![get_payment_item(server.as_ref(), &payment)]
    } else {
        Payment::find_from_user(&client, user)
            .await?
            .iter()
            .map(|p| get_payment_item(server.as_ref(), p))
            .collect()
    };

    Ok(Json(json!({ "payments": payments })).into_response())
}

fn get_payment_item(server: &Server, payment: &Payment) -> serde_json::Value {
    let checkout_link = match payment.processor {
        PaymentProcessor::Paypal => server.paypal.get_checkout_link(payment),
    };

    json!({
        "id": payment.id,
        "createdAt": payment.created_at.format(&Rfc3339).unwrap(),
        "status": payment.status,
        "currency": payment.currency,
        "grossAmount": payment.gross_amount,
        "netAmount": payment.net_amount,
        "fromUser": payment.from_user,
        "toUser": payment.to_user,
        "processor": payment.processor,
        "reference": payment.reference,
        "checkoutLink": checkout_link,
    })
}

/// Body payload for PATCH-request.
#[derive(Deserialize)]
pub struct PatchRequestPayload {
    id: Option<Uuid>,
    reference: Option<String>,
    complete: Option<bool>,
}

/// Handle payment PATCH requests.
pub async fn handle_payment_patch(
    State(server): State<Arc<Server>>,
    WithRejection(Json(payload), _): WithRejection<Json<PatchRequestPayload>, Error>,
) -> Result<Response> {
    let mut client = server.pg_pool.get().await?;

    use Error::*;

    let maybe_payment = if let Some(id) = payload.id {
        Payment::get(&client, id).await?
    } else if let Some(reference) = payload.reference {
        Payment::get_by_reference(&client, reference.as_ref()).await?
    } else {
        None
    };
    let Some(mut payment) = maybe_payment else {
        return Err(PaymentNotFound);
    };

    let complete = payload.complete.unwrap_or_default();
    match payment.processor {
        PaymentProcessor::Paypal => {
            server.paypal.update_payment(&mut payment).await?;
            payment.update(&client).await?;
            if complete {
                server.paypal.complete_payment(&mut payment).await?;
            }
        }
    }

    if complete {
        let net_amount = payment.net_amount.ok_or_else(|| {
            Internal(format!(
                "failed to get net_amount for payment {}",
                payment.id
            ))
        })?;

        let Some(amount) = server
            .currency_converter
            .convert(&payment.currency, net_amount)
            .await?
        else {
            return Err(Internal(format!(
                "failed to convert currency for payment {}",
                payment.id
            )));
        };

        let mut interval = interval(Duration::from_millis(10));
        let mut remains = 100;

        loop {
            interval.tick().await;

            let result = try_top_up_balance_atomically(&mut client, &payment, amount).await;
            if !is_serialization_failure(&result) {
                break result;
            }

            remains -= 1;
            if remains == 0 {
                break result;
            }
        }?;
    }

    Ok(Json(json!({})).into_response())
}

async fn try_top_up_balance_atomically(
    client: &mut Client,
    payment: &Payment,
    amount: Decimal,
) -> Result<()> {
    let tx = client
        .build_transaction()
        .isolation_level(IsolationLevel::RepeatableRead)
        .start()
        .await?;

    use Error::*;
    let status = Payment::get(&tx, payment.id)
        .await?
        .ok_or_else(|| Internal(format!("failed to get payment {}", payment.id)))?
        .status;
    if !matches!(status, PaymentStatus::Approved) {
        return Err(BadPaymentStatus);
    }

    let Some(mut user) = User::get(&tx, payment.to_user).await? else {
        return Err(Internal(format!(
            "failed to get from_user for payment {}",
            payment.id
        )));
    };

    payment.update(&tx).await?;
    user.balance += amount;
    user.update(&tx).await?;
    tx.commit().await?;

    info!("completed payment {}", payment.id);
    Ok(())
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
    gross_amount: Decimal,
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

    let payments = Payment::find_from_user(&client, user).await?;
    if let Some(created_at) = payments.first().map(|p| p.created_at) {
        if created_at > OffsetDateTime::now_utc() - Duration::from_secs(3600) {
            return Err(Error::BadRequest(
                "too frequent payment requests".to_owned(),
            ));
        }
    };

    let mut payment = match payload.processor {
        PaymentProcessor::Paypal => {
            server
                .paypal
                .create_payment(
                    payload.currency,
                    payload.gross_amount,
                    user,
                    payload.to_user.unwrap_or(user),
                    payload.locale.as_deref(),
                )
                .await?
        }
    };

    payment.insert(&client).await?;

    info!(
        "created payment {} of {} {}",
        payment.id, payment.gross_amount, payment.currency
    );
    let item = get_payment_item(server.as_ref(), &payment);
    Ok(Json(json!({ "payment": item })).into_response())
}
