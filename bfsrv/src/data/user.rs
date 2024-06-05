use std::str::FromStr;

use crate::data::Result;
use deadpool_postgres::GenericClient;
use lettre::Address as EmailAddress;
use rust_decimal::Decimal;
use time::OffsetDateTime;
use tokio_postgres::Row;
use uuid::Uuid;

/// User data.
pub struct User {
    pub id: Uuid,
    pub created_at: OffsetDateTime,
    pub email: EmailAddress,
    pub referrer: Option<Uuid>,
    pub campaign: Uuid,
    pub balance: Decimal,
    pub allocated_fee: Decimal,
}

impl User {
    /// Create a new User instance.
    pub fn new(
        email: EmailAddress,
        referrer: Option<Uuid>,
        campaign: Uuid,
        balance: Decimal,
    ) -> Self {
        Self {
            id: Uuid::nil(),
            created_at: OffsetDateTime::UNIX_EPOCH,
            email,
            referrer,
            campaign,
            balance,
            allocated_fee: Decimal::ZERO,
        }
    }

    /// Get a user with a given ID.
    pub async fn get(client: &impl GenericClient, id: Uuid) -> Result<Option<Self>> {
        let stmt = client
            .prepare_cached(
                r#"
                SELECT *
                  FROM "user"
                 WHERE id = $1
                "#,
            )
            .await
            .unwrap();
        let row = client.query_opt(&stmt, &[&id]).await?;
        row.map(Self::from_row).transpose()
    }

    /// Get a user with a given email.
    pub async fn get_by_email(
        client: &impl GenericClient,
        email: &EmailAddress,
    ) -> Result<Option<Self>> {
        let stmt = client
            .prepare_cached(
                r#"
                SELECT *
                  FROM "user"
                 WHERE email = $1
                "#,
            )
            .await
            .unwrap();
        let email_str: &str = email.as_ref();
        let row = client.query_opt(&stmt, &[&email_str]).await?;
        row.map(Self::from_row).transpose()
    }

    /// Insert a new User row and assign ID and created_at.
    pub async fn insert(&mut self, client: &impl GenericClient) -> Result<()> {
        let stmt = client
            .prepare_cached(
                r#"
                INSERT INTO "user"(
                    email,
                    referrer,
                    campaign,
                    balance)
                VALUES ($1, $2, $3, $4)
             RETURNING id, created_at
                "#,
            )
            .await
            .unwrap();

        let email_str: &str = self.email.as_ref();
        let row = client
            .query_one(
                &stmt,
                &[&email_str, &self.referrer, &self.campaign, &self.balance],
            )
            .await?;

        self.id = row.try_get("id")?;
        self.created_at = row.try_get("created_at")?;
        self.allocated_fee = Decimal::ZERO;
        Ok(())
    }

    /// Update user row columns with the current field values.
    pub async fn update(&self, client: &impl GenericClient) -> Result<()> {
        let stmt = client
            .prepare_cached(
                r#"
                UPDATE "user"
                   SET created_at = $2,
                       balance = $3
                 WHERE id = $1
                "#,
            )
            .await
            .unwrap();
        client
            .execute(&stmt, &[&self.id, &self.created_at, &self.balance])
            .await?;
        Ok(())
    }

    /// Decrement user balances with corresponding allocated fees.
    pub async fn update_balances(client: &impl GenericClient) -> Result<()> {
        let stmt = client
            .prepare_cached(
                r#"
                UPDATE "user"
                   SET balance = balance - allocated_fee
                 WHERE allocated_fee > 0 -- use user_allocated_fee_idx
                "#,
            )
            .await
            .unwrap();
        client.execute(&stmt, &[]).await?;
        Ok(())
    }

    /// Clear allocated_fee for every user.
    pub async fn clear_allocated_fees(client: &impl GenericClient) -> Result<()> {
        let stmt = client
            .prepare_cached(
                r#"
                UPDATE "user"
                   SET allocated_fee = 0
                "#,
            )
            .await
            .unwrap();
        client.execute(&stmt, &[]).await?;
        Ok(())
    }

    fn from_row(row: Row) -> Result<Self> {
        let email: &str = row.try_get("email")?;
        Ok(Self {
            id: row.try_get("id")?,
            created_at: row.try_get("created_at")?,
            email: EmailAddress::from_str(email)?,
            referrer: row.try_get("referrer")?,
            campaign: row.try_get("campaign")?,
            balance: row.try_get("balance")?,
            allocated_fee: row.try_get("allocated_fee")?,
        })
    }
}
