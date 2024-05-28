use crate::data::Result;
use deadpool_postgres::GenericClient;
use email_address::EmailAddress;
use std::{net::IpAddr, str::FromStr};
use time::OffsetDateTime;
use tokio_postgres::Row;
use uuid::Uuid;

/// User authentication token.
pub struct Token {
    pub id: Uuid,
    pub created_at: OffsetDateTime,
    pub expires_at: OffsetDateTime,
    pub hash: String,
    pub label: Option<String>,
    pub user: Option<Uuid>,
    pub is_admin: bool,
    pub ip_address: Option<IpAddr>,
    pub email: Option<EmailAddress>,
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
        let email: Option<&str> = row.try_get("email")?;
        Ok(Self {
            id: row.try_get("id")?,
            created_at: row.try_get("created_at")?,
            expires_at: row.try_get("expires_at")?,
            hash: row.try_get("hash")?,
            label: row.try_get("label")?,
            user: row.try_get("user")?,
            is_admin: row.try_get("is_admin")?,
            ip_address: row.try_get("ip_address")?,
            email: email.map(EmailAddress::from_str).transpose()?,
        })
    }
}
