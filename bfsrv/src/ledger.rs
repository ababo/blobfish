use crate::{
    data::{
        capability::{Capability, TaskType},
        node::Node,
        user::User,
    },
    util::fmt::ErrorChainDisplay,
};
use axum::http::StatusCode;
use deadpool_postgres::{Client, Pool as PgPool};
use log::{debug, error};
use rust_decimal::Decimal;
use std::{net::IpAddr, time::Duration};
use tokio::{
    sync::oneshot::{channel, Sender},
    time::interval,
};
use tokio_postgres::{error::SqlState, IsolationLevel};
use uuid::Uuid;

/// Ledger error.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("data")]
    Data(
        #[from]
        #[source]
        crate::data::Error,
    ),
    #[error("deadpool pool")]
    DeadpoolPool(
        #[from]
        #[source]
        deadpool_postgres::PoolError,
    ),
    #[error("node {0} not found")]
    NodeNotFound(Uuid),
    #[error("not enough balance")]
    NotEnoughBalance,
    #[error("postgres")]
    Postgres(
        #[from]
        #[source]
        tokio_postgres::Error,
    ),
    #[error("not enough resources")]
    NotEnoughResources,
    #[error("user {0} not found")]
    UserNotFound(Uuid),
}

impl Error {
    /// HTTP status code.
    pub fn status(&self) -> StatusCode {
        use Error::*;
        match self {
            Data(_) | DeadpoolPool(_) | Postgres(_) | NodeNotFound(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
            UserNotFound(_) => StatusCode::NOT_FOUND,
            NotEnoughBalance => StatusCode::PAYMENT_REQUIRED,
            NotEnoughResources => StatusCode::TOO_MANY_REQUESTS,
        }
    }

    /// Kind code.
    pub fn code(&self) -> &str {
        use Error::*;
        match self {
            Data(err) => err.code(),
            DeadpoolPool(_) => "deadpool_pool",
            NodeNotFound(_) => "node_not_found",
            NotEnoughBalance => "not_enough_balance",
            NotEnoughResources => "not_enough_resources",
            Postgres(_) => "postgres",
            UserNotFound(_) => "user_not_found",
        }
    }
}

/// Ledger result.
pub type Result<T> = std::result::Result<T, Error>;

/// Node usage ledger.
pub struct Ledger {
    pg_pool: PgPool,
    stop_sender: Option<Sender<()>>,
}

impl Ledger {
    /// Create a new Ledger instance.
    pub fn new(pg_pool: PgPool) -> Self {
        let (stop_sender, mut stop_receiver) = channel::<()>();

        let pool_cloned = pg_pool.clone();
        let mut interval = interval(Duration::from_secs(1));
        tokio::spawn(async move {
            interval.tick().await;
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        if let Err(err) = update_balances(&pool_cloned).await {
                            error!("failed to update user balances: {}", ErrorChainDisplay(&err));
                        }
                    },
                    _ = &mut stop_receiver => {
                        debug!("stopped updating user balances");
                        break;
                    }
                }
            }
        });

        Self {
            pg_pool,
            stop_sender: Some(stop_sender),
        }
    }

    /// Allocate a node resource.
    pub async fn allocate(
        &self,
        user: Uuid,
        tariff: &str,
        task_type: TaskType,
    ) -> Result<Allocation> {
        let mut client = self.pg_pool.get().await?;

        let capabilities =
            Capability::find_with_task_type_and_tariff(&client, task_type, tariff).await?;

        let (compute, memory, fee) = capabilities.iter().fold((0, 0, Decimal::ZERO), |acc, cap| {
            (
                acc.0 + cap.compute_load,
                acc.1 + cap.memory_load,
                acc.2 + cap.fee,
            )
        });

        let mut interval = interval(Duration::from_millis(10));
        let mut remains = 10;

        use Error::*;
        let node = loop {
            interval.tick().await;

            let result =
                Self::try_allocate_atomically(&mut client, user, compute, memory, fee).await;
            if !matches!(&result, Err(NotEnoughResources)) && !is_serialization_failure(&result) {
                break result;
            }

            remains -= 1;
            if remains == 0 {
                break result;
            }
        }?;

        let allocation_id = Uuid::new_v4();
        let capability_names: Vec<_> = capabilities.into_iter().map(|n| n.name).collect();
        log::debug!(
            "allocated {allocation_id} ({} on {} for {})",
            capability_names.join(","),
            node.id,
            user
        );

        Ok(Allocation {
            id: allocation_id,
            ip_address: node.ip_address,
            capabilities: capability_names,
            pool: self.pg_pool.clone(),
            user,
            node: node.id,
            compute,
            memory,
            fee,
        })
    }

    async fn try_allocate_atomically(
        client: &mut Client,
        user: Uuid,
        compute: u32,
        memory: u32,
        fee: Decimal,
    ) -> Result<Node> {
        let tx = client
            .build_transaction()
            .isolation_level(IsolationLevel::RepeatableRead)
            .start()
            .await?;

        use Error::*;
        let Some(mut user) = User::get(&tx, user).await? else {
            return Err(UserNotFound(user));
        };

        if !user.balance.is_sign_positive() {
            return Err(Error::NotEnoughBalance);
        }

        let Some(mut node) = Node::find_one_with_available_resources(&tx, compute, memory).await?
        else {
            return Err(NotEnoughResources);
        };

        node.compute_load += compute;
        node.memory_load += memory;
        node.update(&tx).await?;

        user.allocated_fee += fee;
        user.update(&tx).await?;

        tx.commit().await?;
        Ok(node)
    }
}

/// Infsrv node resource allocation.
pub struct Allocation {
    id: Uuid,
    ip_address: IpAddr,
    capabilities: Vec<String>,
    pool: PgPool,
    user: Uuid,
    node: Uuid,
    compute: u32,
    memory: u32,
    fee: Decimal,
}

impl Allocation {
    /// Allocated resource capabilities.
    pub fn capabilities(&self) -> &[String] {
        &self.capabilities
    }

    /// IP address of a node where the resource is allocated.
    pub fn ip_address(&self) -> IpAddr {
        self.ip_address
    }

    /// Check if the resource must be deallocated.
    pub async fn check_invalidated(&self) -> Result<bool> {
        let client = self.pool.get().await?;

        let Some(user) = User::get(&client, self.user).await? else {
            return Err(Error::UserNotFound(self.user));
        };

        Ok(!user.balance.is_sign_positive())
    }

    async fn deallocate(
        pool: PgPool,
        user: Uuid,
        node: Uuid,
        compute: u32,
        memory: u32,
        fee: Decimal,
    ) -> Result<()> {
        let mut client = pool.get().await?;

        let mut interval = interval(Duration::from_millis(10));
        let mut remains = 10000;

        loop {
            interval.tick().await;

            let result =
                Self::try_deallocate_atomically(&mut client, user, node, compute, memory, fee)
                    .await;
            if !is_serialization_failure(&result) {
                break result;
            }

            remains -= 1;
            if remains == 0 {
                break result;
            }
        }
    }

    async fn try_deallocate_atomically(
        client: &mut Client,
        user: Uuid,
        node: Uuid,
        compute: u32,
        memory: u32,
        fee: Decimal,
    ) -> Result<()> {
        let tx = client
            .build_transaction()
            .isolation_level(IsolationLevel::RepeatableRead)
            .start()
            .await?;

        use Error::*;
        let Some(mut node) = Node::get(&tx, node).await? else {
            return Err(NodeNotFound(node));
        };

        node.compute_load -= compute;
        node.memory_load -= memory;
        node.update(&tx).await?;

        let Some(mut user) = User::get(&tx, user).await? else {
            return Err(UserNotFound(user));
        };

        user.allocated_fee -= fee;
        user.update(&tx).await?;

        tx.commit().await?;
        Ok(())
    }
}

impl Drop for Allocation {
    fn drop(&mut self) {
        debug!("deallocating {}", self.id);

        let id = self.id;
        let pool = self.pool.clone();
        let user = self.user;
        let node = self.node;
        let compute = self.compute;
        let memory = self.memory;
        let fee = self.fee;

        tokio::spawn(async move {
            if let Err(err) = Self::deallocate(pool, user, node, compute, memory, fee).await {
                error!("failed to deallocate {id}: {}", ErrorChainDisplay(&err));
            } else {
                debug!("deallocated {id}");
            }
        });
    }
}

impl Drop for Ledger {
    fn drop(&mut self) {
        let stop_sender = self.stop_sender.take().unwrap();
        stop_sender.send(()).unwrap()
    }
}

async fn update_balances(pool: &PgPool) -> Result<()> {
    let client = pool.get().await?;
    User::update_balances(&client).await.map_err(Into::into)
}

#[inline]
fn is_serialization_failure<T>(error: &Result<T>) -> bool {
    const SQL_STATE: Option<&SqlState> = Some(&SqlState::T_R_SERIALIZATION_FAILURE);
    use Error::*;
    matches!(error, Err(Data(crate::data::Error::Postgres(e))) if e.code() == SQL_STATE)
        || matches!(error, Err(Postgres(e)) if e.code() == SQL_STATE)
}
