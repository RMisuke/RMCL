//! 认证域：Microsoft 设备码登录、离线账户、账户列表 / 切换 / 删除 / 刷新令牌。
//!
//! `impl CliContext` 的一个分片（P2-1：把 1,450 行的 God class 按域拆开）。
//! 方法名 / 签名与拆分前完全一致，命令层无需改动。

use super::*;

impl CliContext {
    pub async fn auth_login_microsoft(
        &self,
        progress: Option<rmccc_auth::ProgressCallback>,
    ) -> Result<rmccc_auth::AuthSession> {
        let session = {
            let core = self.core.read().await;
            let account = core
                .account_repo
                .factory("microsoft")?
                .create(rmccc_auth::CreateAccountContext {
                    additional_data: None,
                    progress,
                })
                .await?;
            core.account_repo.save_account(account.as_ref())?;
            account.to_session()
        };
        Ok(session)
    }

    pub async fn auth_login_offline(&self, username: &str) -> Result<rmccc_auth::AuthSession> {
        let session = {
            let core = self.core.read().await;
            let account = core
                .account_repo
                .factory("offline")?
                .create(rmccc_auth::CreateAccountContext {
                    additional_data: Some(json!({ "username": username })),
                    progress: None,
                })
                .await?;
            core.account_repo.save_account(account.as_ref())?;
            account.to_session()
        };
        Ok(session)
    }

    /// Log out the active account.
    pub async fn auth_logout(&self) -> Result<()> {
        let core = self.core.read().await;
        if let Some(active) = core.account_repo.load_active()? {
            core.account_repo.remove(&active.account_id)?;
        }
        Ok(())
    }

    /// Log out a specific account by id.
    pub async fn auth_logout_account(&self, account_id: &str) -> Result<()> {
        let core = self.core.read().await;
        core.account_repo.remove(account_id)
    }

    /// Refresh the active session (None when no active account).
    pub async fn auth_refresh(&self) -> Result<Option<rmccc_auth::AuthSession>> {
        let core = self.core.read().await;
        match core.account_repo.load_active()? {
            Some(s) => Ok(Some(core.refresh_session_if_needed(&s).await?)),
            None => Ok(None),
        }
    }

    /// Refresh a specific account session.
    pub async fn auth_refresh_account(
        &self,
        account_id: &str,
    ) -> Result<Option<rmccc_auth::AuthSession>> {
        let core = self.core.read().await;
        let session = core.account_repo.load(account_id)?;
        match session {
            Some(s) => Ok(Some(core.refresh_session_if_needed(&s).await?)),
            None => Ok(None),
        }
    }

    pub async fn auth_status(&self) -> Result<Option<rmccc_auth::AuthSession>> {
        let core = self.core.read().await;
        core.account_repo.load_active()
    }

    /// List persisted account metadata (plaintext — no tokens ever cross).
    pub async fn auth_list(&self) -> Result<Vec<rmccc_auth::AccountMetadata>> {
        let core = self.core.read().await;
        core.account_repo.list_metadata()
    }

    /// Select an account as the active one.
    pub async fn auth_select(&self, account_id: &str) -> Result<()> {
        let core = self.core.read().await;
        core.account_repo.select(account_id)
    }
}
