use crate::data::Result;
use deadpool_postgres::GenericClient;
use rust_decimal::Decimal;
use time::PrimitiveDateTime;
use tokio_postgres::Row;
use uuid::Uuid;

/// User data.
pub struct User {
    pub id: Uuid,
    pub created_at: PrimitiveDateTime,
    pub balance: Decimal,
    pub allocated_fee: Decimal,
}

impl User {
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

    /// Update user row columns with the current field values.
    pub async fn update(&self, client: &impl GenericClient) -> Result<()> {
        let stmt = client
            .prepare_cached(
                r#"
                UPDATE "user"
                   SET created_at = $2,
                       balance = $3,
                       allocated_fee = $4
                 WHERE id = $1
                "#,
            )
            .await
            .unwrap();
        client
            .execute(
                &stmt,
                &[
                    &self.id,
                    &self.created_at,
                    &self.balance,
                    &self.allocated_fee,
                ],
            )
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
        Ok(Self {
            id: row.try_get("id")?,
            created_at: row.try_get("created_at")?,
            balance: row.try_get("balance")?,
            allocated_fee: row.try_get("allocated_fee")?,
        })
    }
}
