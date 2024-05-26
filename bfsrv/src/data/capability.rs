use crate::data::Result;
use deadpool_postgres::GenericClient;
use postgres_types::{FromSql, ToSql};
use rust_decimal::Decimal;
use tokio_postgres::Row;
use uuid::Uuid;

/// Node task type.
#[derive(Clone, Copy, Debug, ToSql, FromSql)]
#[postgres(name = "task_type", rename_all = "snake_case")]
pub enum TaskType {
    Segment,
    Transcribe,
}

/// Node capability.
pub struct Capability {
    pub id: Uuid,
    pub name: String,
    pub compute_load: u32,
    pub memory_load: u32,
    pub fee: Decimal,
    pub languages: Option<String>,
}

impl Capability {
    /// Find capabilities for a given task type and a tariff.
    pub async fn find_with_task_type_and_tariff(
        client: &impl GenericClient,
        task_type: TaskType,
        tariff: &str,
    ) -> Result<Vec<Self>> {
        let stmt = client
            .prepare_cached(
                r#"
                SELECT capability.*
                  FROM task_type_tariff_capability
                  JOIN capability ON capability = id
                 WHERE task_type = $1 AND tariff = $2
                "#,
            )
            .await
            .unwrap();
        let rows = client.query(&stmt, &[&task_type, &tariff]).await?;
        let result: Result<Vec<_>> = rows.into_iter().map(Self::from_row).collect();
        result
    }

    fn from_row(row: Row) -> Result<Self> {
        Ok(Self {
            id: row.try_get("id")?,
            name: row.try_get("name")?,
            compute_load: row.try_get::<'_, _, i32>("compute_load")? as u32,
            memory_load: row.try_get::<'_, _, i32>("memory_load")? as u32,
            fee: row.try_get("fee")?,
            languages: row.try_get("languages")?,
        })
    }
}
