use crate::store::Store;
use std::{net::IpAddr, str::FromStr, sync::Arc};
use uuid::Uuid;

/// Ledger error.
#[derive(Debug, thiserror::Error)]
pub enum Error {}

impl Error {
    /// Whether it's internal error.
    pub fn is_internal(&self) -> bool {
        // TODO: Check between variants when they are added.
        true
    }
}

/// Ledger result.
pub type Result<T> = std::result::Result<T, Error>;

/// Node usage ledger.
pub struct Ledger<S: Store> {
    _store: S,
}

impl<S: Store> Ledger<S> {
    /// Create a new Ledger instance.
    pub fn new(_store: S) -> Arc<Self> {
        Arc::new(Self { _store })
    }

    /// Allocate a node resource.
    pub async fn allocate(
        self: &Arc<Self>,
        _user: Uuid,
        _tariff: &str,
        task_type: TaskType,
    ) -> Result<Allocation<S>> {
        // TODO: Replace this stub with a proper impl.
        use TaskType::*;
        Ok(Allocation {
            id: Uuid::nil(),
            capabilities: match task_type {
                Segment => vec!["segment-cpu".to_owned()],
                Transcribe => vec!["transcribe-small-cpu".to_owned()],
            },
            ip_address: IpAddr::from_str("127.0.0.1").unwrap(),
            _ledger: self.clone(),
        })
    }

    /// Deallocates a given resource allocation.
    /// Returns true if a corresponding allocation was found and closed.
    pub async fn deallocate(&self, _allocation: Uuid) -> Result<bool> {
        // TODO: Implement this.
        Ok(true)
    }
}

/// Infsrv node task type.
pub enum TaskType {
    Segment,
    Transcribe,
}

/// Infsrv node resource allocation.
pub struct Allocation<S: Store> {
    id: Uuid,
    capabilities: Vec<String>,
    ip_address: IpAddr,
    _ledger: Arc<Ledger<S>>,
}

impl<S: Store> Allocation<S> {
    /// Allocated resource UUID.
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Allocated resource capabilities.
    pub fn capabilities(&self) -> &[String] {
        &self.capabilities
    }

    /// IP address of a node where resource is allocated.
    pub fn ip_address(&self) -> IpAddr {
        self.ip_address
    }

    /// Check if the resource was prematurely deallocated.
    pub async fn check_closed(&self) -> Result<bool> {
        // TODO: Implement this.
        Ok(false)
    }
}

impl<S: Store> Drop for Allocation<S> {
    fn drop(&mut self) {
        let id = self.id;
        let ledger = self._ledger.clone();
        tokio::spawn(async move {
            let _ = ledger.deallocate(id).await;
        });
    }
}
