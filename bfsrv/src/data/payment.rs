use crate::data::Result;
use deadpool_postgres::GenericClient;
use postgres_types::{FromSql, ToSql};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use tokio_postgres::Row;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Serialize, ToSql, FromSql)]
#[postgres(name = "payment_status", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum PaymentStatus {
    New,
    Approved,
    Completed,
    Canceled,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, ToSql, FromSql)]
#[postgres(name = "payment_processor", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum PaymentProcessor {
    Paypal,
}

/// Balance top-up payment.
pub struct Payment {
    pub id: Uuid,
    pub created_at: OffsetDateTime,
    pub status: PaymentStatus,
    pub currency: String,
    pub gross_amount: Decimal,
    pub net_amount: Option<Decimal>,
    pub from_user: Uuid,
    pub to_user: Uuid,
    pub processor: PaymentProcessor,
    pub reference: String,
    pub details: Option<String>,
}

impl Payment {
    /// Create a new Payment instance.
    pub fn new(
        currency: String,
        gross_amount: Decimal,
        from_user: Uuid,
        to_user: Uuid,
        processor: PaymentProcessor,
        reference: String,
    ) -> Self {
        Self {
            id: Uuid::nil(),
            created_at: OffsetDateTime::UNIX_EPOCH,
            status: PaymentStatus::New,
            currency,
            gross_amount,
            net_amount: None,
            from_user,
            to_user,
            processor,
            reference,
            details: None,
        }
    }

    /// Get a payment with a given ID.
    pub async fn get(client: &impl GenericClient, id: Uuid) -> Result<Option<Self>> {
        let stmt = client
            .prepare_cached(
                "
                SELECT *
                  FROM payment
                 WHERE id = $1
                ",
            )
            .await
            .unwrap();
        let row = client.query_opt(&stmt, &[&id]).await?;
        row.map(Self::from_row).transpose()
    }

    /// Get a payment with a given reference.
    pub async fn get_by_reference(
        client: &impl GenericClient,
        reference: &str,
    ) -> Result<Option<Self>> {
        let stmt = client
            .prepare_cached(
                "
                SELECT *
                  FROM payment
                 WHERE reference = $1
                ",
            )
            .await
            .unwrap();
        let row = client.query_opt(&stmt, &[&reference]).await?;
        row.map(Self::from_row).transpose()
    }

    /// Insert a new Payment row and assign ID and created_at.
    pub async fn insert(&mut self, client: &impl GenericClient) -> Result<()> {
        let stmt = client
            .prepare_cached(
                "
                INSERT INTO
                    payment(
                        status,
                        currency,
                        gross_amount,
                        net_amount,
                        from_user,
                        to_user,
                        processor,
                        reference,
                        details)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
             RETURNING id, created_at
                ",
            )
            .await
            .unwrap();

        let row = client
            .query_one(
                &stmt,
                &[
                    &self.status,
                    &self.currency,
                    &self.gross_amount,
                    &self.net_amount,
                    &self.from_user,
                    &self.to_user,
                    &self.processor,
                    &self.reference,
                    &self.details,
                ],
            )
            .await?;

        self.id = row.try_get("id")?;
        self.created_at = row.try_get("created_at")?;
        Ok(())
    }

    /// Find payments from a given user. The payments are sorted by created_at in descending order.
    pub async fn find_from_user(client: &impl GenericClient, user: Uuid) -> Result<Vec<Self>> {
        let stmt = client
            .prepare_cached(
                "
                SELECT *
                  FROM payment
                 WHERE from_user = $1
                 ORDER BY created_at DESC
                ",
            )
            .await
            .unwrap();
        let rows = client.query(&stmt, &[&user]).await?;
        rows.into_iter().map(Self::from_row).collect()
    }

    /// Update payment row columns with the current field values.
    pub async fn update(&self, client: &impl GenericClient) -> Result<()> {
        let stmt = client
            .prepare_cached(
                "
                UPDATE payment
                   SET created_at = $2,
                       status = $3,
                       currency = $4,
                       gross_amount = $5,
                       net_amount = $6,
                       from_user = $7,
                       to_user = $8,
                       processor = $9,
                       reference = $10,
                       details = $11
                 WHERE id = $1
                ",
            )
            .await
            .unwrap();
        client
            .execute(
                &stmt,
                &[
                    &self.id,
                    &self.created_at,
                    &self.status,
                    &self.currency,
                    &self.gross_amount,
                    &self.net_amount,
                    &self.from_user,
                    &self.to_user,
                    &self.processor,
                    &self.reference,
                    &self.details,
                ],
            )
            .await?;
        Ok(())
    }

    fn from_row(row: Row) -> Result<Self> {
        Ok(Self {
            id: row.try_get("id")?,
            created_at: row.try_get("created_at")?,
            status: row.try_get("status")?,
            currency: row.try_get("currency")?,
            gross_amount: row.try_get("gross_amount")?,
            net_amount: row.try_get("net_amount")?,
            from_user: row.try_get("from_user")?,
            to_user: row.try_get("to_user")?,
            processor: row.try_get("processor")?,
            reference: row.try_get("reference")?,
            details: row.try_get("details")?,
        })
    }
}
