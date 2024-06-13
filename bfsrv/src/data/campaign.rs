use crate::data::Result;
use deadpool_postgres::GenericClient;
use rust_decimal::Decimal;
use tokio_postgres::Row;
use uuid::Uuid;

/// Promotional campaign.
pub struct Campaign {
    pub id: Uuid,
    #[allow(dead_code)]
    pub hash: String,
    pub initial_balance: Decimal,
}

impl Campaign {
    /// Find campaign by a given promo code.
    pub async fn find_by_promo_code(
        client: &impl GenericClient,
        promo_code: &str,
    ) -> Result<Option<Self>> {
        let stmt = client
            .prepare_cached(
                "
                SELECT *
                  FROM campaign
                 WHERE hash = crypt($1, hash)
                 LIMIT 1
                ",
            )
            .await
            .unwrap();
        let row = client.query_opt(&stmt, &[&promo_code]).await?;
        row.map(Self::from_row).transpose()
    }

    fn from_row(row: Row) -> Result<Self> {
        Ok(Self {
            id: row.try_get("id")?,
            hash: row.try_get("hash")?,
            initial_balance: row.try_get("initial_balance")?,
        })
    }
}
