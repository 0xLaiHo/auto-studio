use std::future::Future;
use std::pin::Pin;

use crate::agent::AgentRunId;
use crate::project::Project;

pub use crate::error::CreativeRuntimeError;

pub type CreativeRuntimeFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Project, CreativeRuntimeError>> + Send + 'a>>;

/// Application seam used by transports to invoke the Creative Agent runtime.
pub trait CreativeRuntime: Send + Sync {
    fn plan(&self, expected_revision: u64) -> CreativeRuntimeFuture<'_>;
    fn resume_planning(
        &self,
        expected_revision: u64,
        run_id: AgentRunId,
    ) -> CreativeRuntimeFuture<'_>;
    fn execute_approved(
        &self,
        expected_revision: u64,
        run_id: AgentRunId,
    ) -> CreativeRuntimeFuture<'_>;
    fn reconcile_unknown(
        &self,
        expected_revision: u64,
        run_id: AgentRunId,
    ) -> CreativeRuntimeFuture<'_>;
    fn resume_submitted(
        &self,
        expected_revision: u64,
        run_id: AgentRunId,
    ) -> CreativeRuntimeFuture<'_>;
}
