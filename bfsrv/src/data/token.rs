use crate::data::Result;
use deadpool_postgres::GenericClient;
use time::OffsetDateTime;
use tokio_postgres::Row;
use uuid::Uuid;

/// User authentication token.
pub struct Token {
    pub id: Uuid,
    pub hash: String,
    pub created_at: OffsetDateTime,
    pub user: Uuid,
    pub is_admin: bool,
}

impl Token {
    /// Authorize and authenticate user by a given token ID and key.
    pub async fn authorize(
        client: &impl GenericClient,
        token: Uuid,
        key: &str,
    ) -> Result<Option<Token>> {
        let stmt = client
            .prepare_cached(
                "
                SELECT *
                  FROM token
                 WHERE id = $1 AND hash = crypt($2, hash)
                ",
            )
            .await
            .unwrap();
        let row = client.query_opt(&stmt, &[&token, &key]).await?;
        row.map(Self::from_row).transpose()
    }

    fn from_row(row: Row) -> Result<Self> {
        Ok(Self {
            id: row.try_get("id")?,
            created_at: row.try_get("created_at")?,
            hash: row.try_get("hash")?,
            user: row.try_get("user")?,
            is_admin: row.try_get("is_admin")?,
        })
    }
}
