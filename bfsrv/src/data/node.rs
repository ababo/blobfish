use std::net::IpAddr;

use crate::data::Result;
use deadpool_postgres::GenericClient;
use tokio_postgres::Row;
use uuid::Uuid;

/// Worker node (e.g. infsrv).
pub struct Node {
    pub id: Uuid,
    pub label: String,
    pub ip_address: IpAddr,
    pub compute_capacity: u32,
    pub memory_capacity: u32,
    pub compute_load: u32,
    pub memory_load: u32,
}

impl Node {
    /// Find a node with specified resources available.
    pub async fn find_one_with_available_resources(
        client: &impl GenericClient,
        compute: u32,
        memory: u32,
    ) -> Result<Option<Node>> {
        let stmt = client
            .prepare_cached(
                "
				SELECT *
                  FROM node
                 WHERE compute_capacity - compute_load >= $1
                       AND memory_capacity - memory_load >= $2
                 LIMIT 1
                ",
            )
            .await
            .unwrap();
        let row = client
            .query_opt(&stmt, &[&(compute as i32), &(memory as i32)])
            .await?;
        row.map(Self::from_row).transpose()
    }

    /// Get a node with a given ID.
    pub async fn get(client: &impl GenericClient, id: Uuid) -> Result<Option<Self>> {
        let stmt = client
            .prepare_cached(
                "
                SELECT *
                  FROM node
                 WHERE id = $1
                ",
            )
            .await
            .unwrap();
        let row = client.query_opt(&stmt, &[&id]).await?;
        row.map(Self::from_row).transpose()
    }

    /// Update node row columns with the current field values.
    pub async fn update(&self, client: &impl GenericClient) -> Result<()> {
        let stmt = client
            .prepare_cached(
                "
                UPDATE node
                   SET label = $2,
                       ip_address = $3,
                       compute_capacity = $4,
                       memory_capacity = $5,
                       compute_load = $6,
                       memory_load = $7
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
                    &self.label,
                    &self.ip_address,
                    &(self.compute_capacity as i32),
                    &(self.memory_capacity as i32),
                    &(self.compute_load as i32),
                    &(self.memory_load as i32),
                ],
            )
            .await?;
        Ok(())
    }

    fn from_row(row: Row) -> Result<Self> {
        Ok(Self {
            id: row.try_get("id")?,
            label: row.try_get("label")?,
            ip_address: row.try_get("ip_address")?,
            compute_capacity: row.try_get::<'_, _, i32>("compute_capacity")? as u32,
            memory_capacity: row.try_get::<'_, _, i32>("memory_capacity")? as u32,
            compute_load: row.try_get::<'_, _, i32>("compute_load")? as u32,
            memory_load: row.try_get::<'_, _, i32>("memory_load")? as u32,
        })
    }
}
