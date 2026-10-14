use std::sync::Arc;

use crate::agents::AgentManager;
use crate::core::store::Store;
use crate::github::GithubService;
use crate::lsp::manager::LspManager;

pub struct AppState {
    pub store: Arc<Store>,
    pub agents: Arc<AgentManager>,
    pub github: Arc<GithubService>,
    pub lsp: LspManager,
}
