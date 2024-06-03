use crate::data::Result;
use deadpool_postgres::GenericClient;
use lettre::Address as EmailAddress;
use std::{net::IpAddr, str::FromStr};
use time::OffsetDateTime;
use tokio_postgres::Row;
use uuid::Uuid;

/// Token secret key length.
pub const TOKEN_KEY_LEN: usize = 32;

/// Token secret key to be stored as hash.
pub type TokenKey = [u8; TOKEN_KEY_LEN];

/// User authentication token.
pub struct Token {
    pub id: Uuid,
    pub created_at: OffsetDateTime,
    pub expires_at: OffsetDateTime,
    pub hash: String,
    pub label: Option<String>,
    pub user: Option<Uuid>,
    pub is_admin: bool,
    pub ip_address: IpAddr,
    pub email: Option<EmailAddress>,
}

impl Token {
    /// Create a new Token instance.
    pub fn new(
        expires_at: OffsetDateTime,
        label: Option<String>,
        user: Option<Uuid>,
        is_admin: bool,
        ip_address: IpAddr,
        email: Option<EmailAddress>,
    ) -> Self {
        Self {
            id: Uuid::nil(),
            created_at: OffsetDateTime::UNIX_EPOCH,
            expires_at,
            hash: String::new(),
            label,
            user,
            is_admin,
            ip_address,
            email,
        }
    }

    /// Get and authenticate a token with a given ID.
    pub async fn get_and_authenticate(
        client: &impl GenericClient,
        id: Uuid,
        key: TokenKey,
    ) -> Result<Option<Token>> {
        let stmt = client
            .prepare_cached(
                "
                SELECT *
                  FROM token
                 WHERE id = $1 AND hash = crypt($2::bytea::text, hash)
                ",
            )
            .await
            .unwrap();
        let row = client.query_opt(&stmt, &[&id, &key.as_slice()]).await?;
        row.map(Self::from_row).transpose()
    }

    /// Find last token with a given ip_address.
    pub async fn find_last_with_ip_address(
        client: &impl GenericClient,
        ip_address: IpAddr,
    ) -> Result<Option<Self>> {
        let stmt = client
            .prepare_cached(
                "
                SELECT *
                  FROM token
                 WHERE ip_address = $1
                 ORDER BY created_at DESC
                 LIMIT 1
                ",
            )
            .await
            .unwrap();
        let row = client.query_opt(&stmt, &[&ip_address]).await?;
        row.map(Self::from_row).transpose()
    }

    /// Insert a new Token row and assign ID, created_at and hash.
    pub async fn insert(&mut self, client: &impl GenericClient) -> Result<TokenKey> {
        let stmt = client
            .prepare_cached(
                r#"
                WITH key AS (
                    SELECT gen_random_bytes($1)
                ), hash AS (
                    SELECT crypt((SELECT * FROM key)::text, gen_salt('bf'))
                )
                INSERT INTO token(
                    expires_at,
                    hash,
                    label,
                    "user",
                    is_admin,
                    ip_address,
                    email)
                VALUES ($2, (SELECT * FROM hash), $3, $4, $5, $6, $7)
             RETURNING id, created_at, hash, (SELECT * FROM key) AS key
                "#,
            )
            .await
            .unwrap();

        let row = client
            .query_one(
                &stmt,
                &[
                    &(TOKEN_KEY_LEN as i32),
                    &self.expires_at,
                    &self.label,
                    &self.user,
                    &self.is_admin,
                    &self.ip_address,
                    &self
                        .email
                        .as_ref()
                        .map(<EmailAddress as AsRef<str>>::as_ref),
                ],
            )
            .await?;

        self.id = row.try_get("id")?;
        self.created_at = row.try_get("created_at")?;
        self.hash = row.try_get("hash")?;

        let token: Vec<u8> = row.try_get("key")?;
        Ok(token.try_into().unwrap())
    }

    /// Update token row columns with the current field values.
    pub async fn update(&self, client: &impl GenericClient) -> Result<()> {
        let stmt = client
            .prepare_cached(
                r#"
                UPDATE token
                   SET created_at = $2,
                       expires_at = $3,
                       hash = $4,
                       label = $5,
                       "user" = $6,
                       is_admin = $7,
                       ip_address = $8,
                       email = $9
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
                    &self.expires_at,
                    &self.hash,
                    &self.label,
                    &self.user,
                    &self.is_admin,
                    &self.ip_address,
                    &self
                        .email
                        .as_ref()
                        .map(<EmailAddress as AsRef<str>>::as_ref),
                ],
            )
            .await?;
        Ok(())
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
