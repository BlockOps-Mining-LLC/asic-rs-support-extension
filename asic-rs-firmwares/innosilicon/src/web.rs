//! pyasic 0.79 `web/innosilicon.py` read contract: POST requests query
//! read endpoints. No reboot, poweroff, restart, or pool updates are exposed.
use asic_rs_core::traits::miner::{ExposeSecret, MinerAuth, SecretString};
use reqwest::{Client, StatusCode};
use serde_json::{Value, json};
use std::{net::IpAddr, time::Duration};
use tokio::sync::Mutex;

#[derive(Debug)]
pub struct InnosiliconWebAPI {
    ip: IpAddr,
    auth: MinerAuth,
    client: Result<Client, reqwest::Error>,
    token: Mutex<Option<SecretString>>,
}
impl InnosiliconWebAPI {
    pub fn new(ip: IpAddr, auth: MinerAuth) -> Self {
        Self {
            ip,
            auth,
            client: Client::builder()
                .connect_timeout(Duration::from_secs(5))
                .timeout(Duration::from_secs(5))
                .redirect(reqwest::redirect::Policy::none())
                .build(),
            token: Mutex::new(None),
        }
    }
    pub fn set_auth(&mut self, auth: MinerAuth) {
        self.auth = auth;
        self.token = Mutex::new(None);
    }
    pub fn allows_read(command: &str) -> bool {
        matches!(
            command,
            "type" | "getAll" | "overview" | "summary" | "getErrorDetail" | "pools"
        )
    }
    fn client(&self) -> anyhow::Result<&Client> {
        self.client
            .as_ref()
            .map_err(|_| anyhow::anyhow!("Cannot initialize Innosilicon read client"))
    }
    async fn login(&self) -> anyhow::Result<SecretString> {
        if let MinerAuth::TokenAuth(token) = &self.auth {
            return Ok(token.clone());
        }
        let response = self
            .client()?
            .post(format!("http://{}/api/auth", self.ip))
            .form(&[
                ("username", self.auth.username()),
                ("password", self.auth.password()),
            ])
            .send()
            .await
            .map_err(|_| anyhow::anyhow!("Innosilicon read-session login failed"))?
            .error_for_status()
            .map_err(|_| anyhow::anyhow!("Innosilicon read-session login denied"))?;
        let value: Value = response
            .json()
            .await
            .map_err(|_| anyhow::anyhow!("Invalid Innosilicon read-session response"))?;
        let token = value
            .get("jwt")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| anyhow::anyhow!("Innosilicon read-session token missing"))?;
        Ok(SecretString::from(token.to_string()))
    }
    pub async fn read(&self, command: &str) -> anyhow::Result<Value> {
        anyhow::ensure!(
            Self::allows_read(command),
            "Unsupported Innosilicon read command"
        );
        let mut token = self.token.lock().await;
        if token.is_none() {
            *token = Some(self.login().await?);
        }
        for attempt in 0..2 {
            let bearer = token
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Innosilicon read token missing"))?;
            let response = self
                .client()?
                .post(format!("http://{}/api/{command}", self.ip))
                .bearer_auth(bearer.expose_secret())
                .json(&json!({}))
                .send()
                .await
                .map_err(|_| anyhow::anyhow!("Innosilicon read request failed"))?;
            if matches!(
                response.status(),
                StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
            ) && attempt == 0
                && !matches!(self.auth, MinerAuth::TokenAuth(_))
            {
                *token = Some(self.login().await?);
                continue;
            }
            let value: Value = response
                .error_for_status()
                .map_err(|_| anyhow::anyhow!("Innosilicon read request denied"))?
                .json()
                .await
                .map_err(|_| anyhow::anyhow!("Invalid Innosilicon read response"))?;
            if value.get("token").and_then(Value::as_str) == Some("expired")
                && attempt == 0
                && !matches!(self.auth, MinerAuth::TokenAuth(_))
            {
                *token = Some(self.login().await?);
                continue;
            }
            anyhow::ensure!(
                value.get("success").and_then(Value::as_bool) != Some(false),
                "Innosilicon read API reported failure"
            );
            return Ok(value);
        }
        anyhow::bail!("Innosilicon read authentication failed")
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn post_read_allowlist_excludes_mutations() {
        assert!(InnosiliconWebAPI::allows_read("getAll"));
        for command in [
            "poweroff",
            "reboot",
            "restartCgMiner",
            "updatePools",
            "auth",
            "setPower",
        ] {
            assert!(!InnosiliconWebAPI::allows_read(command));
        }
    }
}
