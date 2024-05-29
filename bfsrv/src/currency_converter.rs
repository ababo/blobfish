use axum::http::StatusCode;
use log::debug;
use reqwest::Client;
use rust_decimal::Decimal;
use serde::Deserialize;
use std::{collections::HashMap, sync::RwLock, time::Duration};
use time::OffsetDateTime;

/// CurrencyConverter error.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("reqwest")]
    Reqwest(
        #[from]
        #[source]
        reqwest::Error,
    ),
}

impl Error {
    /// HTTP status code.
    pub fn status(&self) -> StatusCode {
        use Error::*;
        match self {
            Reqwest(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// Kind code.
    pub fn code(&self) -> &str {
        use Error::*;
        match self {
            Reqwest(_) => "reqwest",
        }
    }
}

/// CurrencyConverter result.
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Deserialize)]
struct RatesResponse {
    rates: HashMap<String, Decimal>,
}

struct State {
    rates: HashMap<String, Decimal>,
    updated_at: OffsetDateTime,
}

/// Currency converter.
pub struct CurrencyConverter {
    base: String,
    state: RwLock<State>,
}

impl CurrencyConverter {
    /// Create a new CurrencyConverter instance for a given base currency.
    pub fn new(base: String) -> Self {
        Self {
            base,
            state: RwLock::new(State {
                rates: HashMap::new(),
                updated_at: OffsetDateTime::UNIX_EPOCH,
            }),
        }
    }

    /// Convert amount from one currency to another.
    pub async fn convert(&self, currency: &str, amount: Decimal) -> Result<Option<Decimal>> {
        {
            let state = self.state.read().unwrap();
            if OffsetDateTime::now_utc() < state.updated_at + Duration::from_secs(24 * 3600) {
                return Ok(state.rates.get(currency).map(|r| amount / *r));
            }
        }

        let response = Client::default()
            .get(format!(
                "https://api.exchangerate-api.com/v4/latest/{}",
                &self.base
            ))
            .send()
            .await?
            .error_for_status()?;

        let response: RatesResponse = response.json().await?;

        let mut state = self.state.write().unwrap();
        state.rates = response.rates;
        state.updated_at = OffsetDateTime::now_utc();

        debug!("retrieved currency rates");
        Ok(state.rates.get(currency).map(|r| amount / *r))
    }
}
