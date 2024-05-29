use axum::http::StatusCode;
use log::debug;
use reqwest::Client;
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::json;
use std::{sync::RwLock, time::Duration};
use time::OffsetDateTime;
use url::Url;

use crate::data::payment::PaymentStatus;

/// Server error.
#[derive(Debug, thiserror::Error)]
pub enum Error {
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
            Reqwest(_) | SerdeJson(_) => StatusCode::INTERNAL_SERVER_ERROR,
            UnsupportedCurrency | UnsupportedLocale => StatusCode::BAD_REQUEST,
        }
    }

    /// Kind code.
    pub fn code(&self) -> &str {
        use Error::*;
        match self {
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
struct TokenResponse {
    access_token: String,
    expires_in: u64,
}

#[derive(Deserialize)]
struct OrderResponse {
    id: String,
    status: String,
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
        "ar_EG", "cs_CZ", "da_DK", "de_DE", "en_AU", "en_GB", "en_US", "es_ES", "es_XC", "fr_FR",
        "fr_XC", "it_IT", "ja_JP", "ko_KR", "nl_NL", "pl_PL", "pt_BR", "ru_RU", "sv_SE", "zh_CN",
        "zh_TW", "zh_XC",
    ];

    /// Initiate a new payment.
    pub async fn initiate(
        &self,
        currency: &str,
        amount: Decimal,
        locale: Option<&str>,
    ) -> Result<(String, Url)> {
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
                    "value": amount,
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

        let response: OrderResponse = response.json().await?;

        let mut url = Url::parse(if self.sandbox {
            "https://www.sandbox.paypal.com/checkoutnow"
        } else {
            "https://www.paypal.com/checkoutnow"
        })
        .unwrap();
        url.query_pairs_mut().append_pair("token", &response.id);

        Ok((response.id, url))
    }

    /// Retrieve a payment status and details by a given reference.
    pub async fn retrieve_status(
        &self,
        reference: &str,
    ) -> Result<(PaymentStatus, Option<String>)> {
        let token = self.get_token().await?;
        let response = Client::default()
            .post(if self.sandbox {
                format!("https://api.sandbox.paypal.com/v2/checkout/orders/{reference}/capture")
            } else {
                format!("https://api.paypal.com/v2/checkout/orders/{reference}/capture")
            })
            .bearer_auth(token)
            .json(&())
            .send()
            .await?
            .error_for_status()?;

        let json = response.text().await?;
        let response: OrderResponse = serde_json::from_str(&json)?;

        use PaymentStatus::*;
        let status = match response.status.as_str() {
            "REVERSED" => Canceled,
            "COMPLETED" => Completed,
            _ => New,
        };

        Ok((status, Some(json)))
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

        let response: TokenResponse = response.json().await?;

        let mut state = self.state.write().unwrap();
        state.token = response.access_token;
        state.token_expires_at =
            OffsetDateTime::now_utc() + Duration::from_secs(response.expires_in);

        debug!(
            "retrieved paypal token (expires at {})",
            state.token_expires_at
        );
        Ok(state.token.clone())
    }
}
