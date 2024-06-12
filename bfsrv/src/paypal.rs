use axum::http::StatusCode;
use log::debug;
use reqwest::Client;
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::json;
use std::{sync::RwLock, time::Duration};
use time::OffsetDateTime;
use url::Url;
use uuid::Uuid;

use crate::data::payment::{Payment, PaymentProcessor, PaymentStatus};

/// Server error.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("bad payment status")]
    BadPaymentStatus,
    #[error("reqwest")]
    Reqwest(
        #[from]
        #[source]
        reqwest::Error,
    ),
    #[error("serde_json")]
    SerdeJson(
        #[from]
        #[source]
        serde_json::Error,
    ),
    #[error("unsupported currency")]
    UnsupportedCurrency,
    #[error("unsupported locale")]
    UnsupportedLocale,
}

impl Error {
    /// HTTP status code.
    pub fn status(&self) -> StatusCode {
        use Error::*;
        match self {
            BadPaymentStatus => StatusCode::UNPROCESSABLE_ENTITY,
            Reqwest(_) | SerdeJson(_) => StatusCode::INTERNAL_SERVER_ERROR,
            UnsupportedCurrency | UnsupportedLocale => StatusCode::BAD_REQUEST,
        }
    }

    /// Kind code.
    pub fn code(&self) -> &str {
        use Error::*;
        match self {
            BadPaymentStatus => "bad_payment_status",
            Reqwest(_) => "reqwest",
            SerdeJson(_) => "serde_json",
            UnsupportedCurrency => "unsupported_currency",
            UnsupportedLocale => "unsupported_locale",
        }
    }
}

/// Server result.
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Deserialize)]
struct TokenResponsePayload {
    access_token: String,
    expires_in: u64,
}

#[derive(Deserialize)]
struct OrderResponsePayload {
    id: String,
    status: String,
    #[serde(default)]
    purchase_units: Vec<PurchaseUnits>,
}

impl OrderResponsePayload {
    fn net_amount(&self) -> Option<Decimal> {
        self.purchase_units
            .first()
            .and_then(|u| u.payments.as_ref().and_then(|p| p.captures.first()))
            .map(|c| c.seller_receivable_breakdown.net_amount.value)
    }
}

#[derive(Deserialize)]
struct PurchaseUnits {
    payments: Option<Payments>,
}

#[derive(Deserialize)]
struct Payments {
    captures: Vec<Capture>,
}

#[derive(Deserialize)]
struct Capture {
    seller_receivable_breakdown: SellerReceivableBreakdown,
}

#[derive(Deserialize)]
struct SellerReceivableBreakdown {
    net_amount: Amount,
}

#[derive(Deserialize)]
struct Amount {
    value: Decimal,
}

struct State {
    token: String,
    token_expires_at: OffsetDateTime,
}

/// Paypal payment processor.
pub struct PaypalProcessor {
    sandbox: bool,
    client_id: String,
    secret_key: String,
    return_url: Url,
    cancel_url: Url,
    state: RwLock<State>,
}

impl PaypalProcessor {
    /// Create a new PaypalProcessor instance.
    pub fn new(
        sandbox: bool,
        client_id: String,
        secret_key: String,
        return_url: Url,
        cancel_url: Url,
    ) -> Self {
        Self {
            sandbox,
            client_id,
            secret_key,
            return_url,
            cancel_url,
            state: RwLock::new(State {
                token: String::new(),
                token_expires_at: OffsetDateTime::UNIX_EPOCH,
            }),
        }
    }

    const CURRENCIES: &'static [&'static str] = &[
        "AUD", "BRL", "CAD", "CNY", "CZK", "DKK", "EUR", "HKD", "HUF", "ILS", "JPY", "MYR", "MXN",
        "TWD", "NZD", "NOK", "PHP", "PLN", "GBP", "RUB", "SGD", "SEK", "CHF", "THB", "USD",
    ];

    const LOCALES: &'static [&'static str] = &[
        "ar-EG", "cs-CZ", "da-DK", "de-DE", "en-AU", "en-GB", "en-US", "es-ES", "es-XC", "fr-FR",
        "fr-XC", "it-IT", "ja-JP", "ko-KR", "nl-NL", "pl-PL", "pt-BR", "ru-RU", "sv-SE", "zh-CN",
        "zh-TW", "zh-XC",
    ];

    /// Register a new payment.
    pub async fn create_payment(
        &self,
        currency: String,
        gross_amount: Decimal,
        from_user: Uuid,
        to_user: Uuid,
        locale: Option<&str>,
    ) -> Result<Payment> {
        use Error::*;
        if !Self::CURRENCIES.iter().any(|c| *c == currency) {
            return Err(UnsupportedCurrency);
        }

        if let Some(code) = locale {
            if !Self::LOCALES.iter().any(|c| *c == code) {
                return Err(UnsupportedLocale);
            }
        }

        let request = json!({
            "intent": "CAPTURE",
            "purchase_units": [{
                "amount": {
                    "currency_code": currency,
                    "value": gross_amount,
                }
            }],
            "payment_source": {
                "paypal": {
                    "experience_context": {
                        "payment_method_preference": "IMMEDIATE_PAYMENT_REQUIRED",
                        "brand_name": "Blobfish",
                        "locale": locale.unwrap_or("en-US"),
                        "landing_page": "LOGIN",
                        "shipping_preference": "NO_SHIPPING",
                        "user_action": "PAY_NOW",
                        "return_url": self.return_url,
                        "cancel_url": self.cancel_url,
                    }
                }
            }
        });

        let token = self.get_token().await?;
        let response = Client::default()
            .post(if self.sandbox {
                "https://api.sandbox.paypal.com/v2/checkout/orders"
            } else {
                "https://api.paypal.com/v2/checkout/orders"
            })
            .bearer_auth(token)
            .json(&request)
            .send()
            .await?
            .error_for_status()?;

        let json = response.text().await?;
        let payload: OrderResponsePayload = serde_json::from_str(&json)?;

        Ok(Payment::new(
            currency,
            gross_amount,
            from_user,
            to_user,
            PaymentProcessor::Paypal,
            payload.id,
        ))
    }

    /// Update status for a given payment.
    pub async fn update_payment(&self, payment: &mut Payment) -> Result<()> {
        let token = self.get_token().await?;
        let response = Client::default()
            .get(self.get_order_link(&payment.reference, false))
            .bearer_auth(token)
            .send()
            .await?
            .error_for_status()?;

        let json = response.text().await?;
        let payload: OrderResponsePayload = serde_json::from_str(&json)?;

        use PaymentStatus::*;
        let status = match payload.status.as_str() {
            "APPROVED" => Approved,
            "COMPLETED" => Completed,
            "REVERSED" => Canceled,
            _ => New,
        };

        payment.status = status;
        payment.net_amount = payload.net_amount();
        payment.details = Some(json);
        Ok(())
    }

    fn get_order_link(&self, reference: &str, capture: bool) -> String {
        let mut url = if self.sandbox {
            format!("https://api.sandbox.paypal.com/v2/checkout/orders/{reference}")
        } else {
            format!("https://api.paypal.com/v2/checkout/orders/{reference}")
        };
        if capture {
            url += "/capture"
        }
        url
    }

    /// Complete a given approved payment.
    pub async fn complete_payment(&self, payment: &mut Payment) -> Result<()> {
        use PaymentStatus::*;
        if !matches!(payment.status, Approved) {
            return Err(Error::BadPaymentStatus);
        }

        let token = self.get_token().await?;
        let response = Client::default()
            .post(self.get_order_link(&payment.reference, true))
            .bearer_auth(token)
            .json(&())
            .send()
            .await?
            .error_for_status()?;

        let json = response.text().await?;
        let payload: OrderResponsePayload = serde_json::from_str(&json)?;

        payment.status = Completed;
        payment.net_amount = payload.net_amount();
        payment.details = Some(json);
        Ok(())
    }

    /// Get a URL for user to follow for a payment completion.
    pub fn get_checkout_link(&self, payment: &Payment) -> Option<Url> {
        if !matches!(payment.status, PaymentStatus::New) {
            return None;
        }

        let mut url = Url::parse(if self.sandbox {
            "https://www.sandbox.paypal.com/checkoutnow"
        } else {
            "https://www.paypal.com/checkoutnow"
        })
        .unwrap();

        url.query_pairs_mut()
            .append_pair("token", &payment.reference);

        Some(url)
    }

    async fn get_token(&self) -> Result<String> {
        {
            let state = self.state.read().unwrap();
            if OffsetDateTime::now_utc() < state.token_expires_at {
                return Ok(state.token.clone());
            }
        }

        let response = Client::default()
            .post(if self.sandbox {
                "https://api.sandbox.paypal.com/v1/oauth2/token"
            } else {
                "https://api.paypal.com/v1/oauth2/token"
            })
            .basic_auth(&self.client_id, Some(&self.secret_key))
            .body("grant_type=client_credentials")
            .send()
            .await?
            .error_for_status()?;

        let payload: TokenResponsePayload = response.json().await?;

        let mut state = self.state.write().unwrap();
        state.token = payload.access_token;
        state.token_expires_at =
            OffsetDateTime::now_utc() + Duration::from_secs(payload.expires_in);

        debug!(
            "retrieved paypal token (expires at {})",
            state.token_expires_at
        );
        Ok(state.token.clone())
    }
}
