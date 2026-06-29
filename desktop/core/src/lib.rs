use chrono::{DateTime, Local, Utc};
use fs2::FileExt;
use keyring::Entry;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::time::sleep;
use uuid::Uuid;

pub const APP_ID: &str = "com.elizabeth.daily-email-cleanout";
pub const KEYCHAIN_SERVICE: &str = "Daily Email Cleanout";
pub const KEYCHAIN_ACCOUNT: &str = "microsoft-refresh-token";
pub const FIXED_PREFIX: &str = "info@-----";
const GRAPH_BASE: &str = "https://graph.microsoft.com/v1.0";
const AUTHORITY: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0";
const SCOPES: &str = "Mail.ReadWrite User.Read offline_access";

#[derive(Debug, Error)]
pub enum AppError {
    #[error("{0}")]
    Message(String),
    #[error("Microsoft authorization is required")]
    AuthorizationRequired,
    #[error("A cleanup is already running")]
    AlreadyRunning,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    #[error(transparent)]
    Keyring(#[from] keyring::Error),
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

pub type Result<T> = std::result::Result<T, AppError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub client_id: String,
    pub dry_run: bool,
    pub schedule_enabled: bool,
    pub run_at_login: bool,
    pub run_on_startup: bool,
    pub run_hour: u8,
    pub run_minute: u8,
    pub account_email: Option<String>,
    pub account_name: Option<String>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            client_id: String::new(),
            dry_run: true,
            schedule_enabled: true,
            run_at_login: true,
            run_on_startup: true,
            run_hour: 8,
            run_minute: 0,
            account_email: None,
            account_name: None,
        }
    }
}

impl AppSettings {
    pub fn validate(&self) -> Result<()> {
        if !self.client_id.is_empty() {
            Uuid::parse_str(&self.client_id)
                .map_err(|_| AppError::Message("Client ID must be a valid UUID".into()))?;
        }
        if self.run_hour > 23 || self.run_minute > 59 {
            return Err(AppError::Message(
                "Daily run time must be a valid local time".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceCodePrompt {
    pub user_code: String,
    pub verification_uri: String,
    pub message: String,
    pub expires_in: u64,
}

#[derive(Debug, Clone)]
pub struct DeviceSession {
    pub device_code: String,
    pub interval: u64,
    pub deadline: Instant,
    pub client_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountSummary {
    pub display_name: String,
    pub email: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchRecord {
    pub id: String,
    pub sender: String,
    pub subject: String,
    pub received_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupResult {
    pub scanned: usize,
    pub matched: usize,
    pub deleted: usize,
    pub failed: usize,
    pub dry_run: bool,
    pub matches: Vec<MatchRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunRecord {
    pub id: String,
    pub trigger: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub status: String,
    pub result: Option<CleanupResult>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum CleanupEvent {
    Started {
        dry_run: bool,
        trigger: String,
    },
    ScanProgress {
        scanned: usize,
    },
    Matched {
        record: MatchRecord,
    },
    ScanComplete {
        scanned: usize,
        matched: usize,
    },
    DeleteProgress {
        deleted: usize,
        failed: usize,
        total: usize,
    },
    Finished {
        result: CleanupResult,
    },
    Failed {
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleStatus {
    pub enabled: bool,
    pub loaded: bool,
    pub label: String,
    pub next_run_description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppStateView {
    pub settings: AppSettings,
    pub signed_in: bool,
    pub fixed_prefix: String,
    pub schedule: ScheduleStatus,
    pub latest_run: Option<RunRecord>,
}

#[derive(Debug, Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    interval: Option<u64>,
    message: String,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GraphUser {
    display_name: String,
    mail: Option<String>,
    user_principal_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GraphPage {
    value: Vec<GraphMessage>,
    #[serde(rename = "@odata.nextLink")]
    next_link: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GraphMessage {
    id: String,
    subject: Option<String>,
    from: Option<GraphSender>,
    received_date_time: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GraphSender {
    email_address: GraphEmailAddress,
}

#[derive(Debug, Deserialize)]
struct GraphEmailAddress {
    address: String,
}

pub fn app_support_dir() -> Result<PathBuf> {
    let base = dirs::data_dir()
        .ok_or_else(|| AppError::Message("Could not locate Application Support".into()))?;
    Ok(base.join(APP_ID))
}

pub fn settings_path() -> Result<PathBuf> {
    Ok(app_support_dir()?.join("settings.json"))
}

pub fn load_settings() -> Result<AppSettings> {
    let path = settings_path()?;
    if !path.exists() {
        return Ok(AppSettings::default());
    }
    let settings: AppSettings = serde_json::from_str(&fs::read_to_string(path)?)?;
    settings.validate()?;
    Ok(settings)
}

pub fn save_settings(settings: &AppSettings) -> Result<()> {
    settings.validate()?;
    let path = settings_path()?;
    ensure_parent(&path)?;
    let temp = path.with_extension("json.tmp");
    fs::write(&temp, serde_json::to_vec_pretty(settings)?)?;
    fs::rename(temp, path)?;
    Ok(())
}

fn ensure_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

fn keychain_entry() -> Result<Entry> {
    Ok(Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT)?)
}

pub fn has_refresh_token() -> bool {
    keychain_entry()
        .and_then(|entry| entry.get_password().map_err(AppError::from))
        .map(|value| !value.is_empty())
        .unwrap_or(false)
}

pub fn clear_refresh_token() -> Result<()> {
    let entry = keychain_entry()?;
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn save_refresh_token(token: &str) -> Result<()> {
    keychain_entry()?.set_password(token)?;
    Ok(())
}

pub async fn begin_device_sign_in(client_id: &str) -> Result<(DeviceCodePrompt, DeviceSession)> {
    Uuid::parse_str(client_id)
        .map_err(|_| AppError::Message("Client ID must be a valid UUID".into()))?;
    let response = Client::new()
        .post(format!("{AUTHORITY}/devicecode"))
        .form(&[("client_id", client_id), ("scope", SCOPES)])
        .send()
        .await?;
    let status = response.status();
    let body = response.text().await?;
    if !status.is_success() {
        return Err(AppError::Message(format!(
            "Microsoft could not begin sign-in: {body}"
        )));
    }
    let payload: DeviceCodeResponse = serde_json::from_str(&body)?;
    let prompt = DeviceCodePrompt {
        user_code: payload.user_code,
        verification_uri: payload.verification_uri,
        message: payload.message,
        expires_in: payload.expires_in,
    };
    let session = DeviceSession {
        device_code: payload.device_code,
        interval: payload.interval.unwrap_or(5).max(1),
        deadline: Instant::now() + Duration::from_secs(payload.expires_in),
        client_id: client_id.to_string(),
    };
    Ok((prompt, session))
}

pub async fn complete_device_sign_in(session: DeviceSession) -> Result<AccountSummary> {
    let client = Client::new();
    let mut interval = session.interval;
    while Instant::now() < session.deadline {
        sleep(Duration::from_secs(interval)).await;
        let response = client
            .post(format!("{AUTHORITY}/token"))
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("client_id", session.client_id.as_str()),
                ("device_code", session.device_code.as_str()),
            ])
            .send()
            .await?;
        let payload: TokenResponse = response.json().await?;
        if let Some(access_token) = payload.access_token {
            let refresh_token = payload.refresh_token.ok_or_else(|| {
                AppError::Message("Microsoft did not return an offline refresh token".into())
            })?;
            save_refresh_token(&refresh_token)?;
            return fetch_account(&client, &access_token).await;
        }
        match payload.error.as_deref() {
            Some("authorization_pending") => continue,
            Some("slow_down") => {
                interval += 5;
                continue;
            }
            Some("authorization_declined") => {
                return Err(AppError::Message("Microsoft sign-in was declined".into()))
            }
            Some("expired_token") => {
                return Err(AppError::Message(
                    "The Microsoft sign-in code expired".into(),
                ))
            }
            _ => {
                return Err(AppError::Message(
                    payload
                        .error_description
                        .unwrap_or_else(|| "Microsoft sign-in failed".into()),
                ))
            }
        }
    }
    Err(AppError::Message(
        "The Microsoft sign-in code expired".into(),
    ))
}

async fn refresh_access_token(client: &Client, settings: &AppSettings) -> Result<String> {
    let refresh_token = keychain_entry()?
        .get_password()
        .map_err(|error| match error {
            keyring::Error::NoEntry => AppError::AuthorizationRequired,
            other => AppError::Keyring(other),
        })?;
    let response = client
        .post(format!("{AUTHORITY}/token"))
        .form(&[
            ("client_id", settings.client_id.as_str()),
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token.as_str()),
            ("scope", SCOPES),
        ])
        .send()
        .await?;
    let payload: TokenResponse = response.json().await?;
    let access_token = payload.access_token.ok_or_else(|| {
        AppError::Message(
            payload
                .error_description
                .unwrap_or_else(|| "Microsoft authorization must be renewed".into()),
        )
    })?;
    if let Some(rotated) = payload.refresh_token {
        save_refresh_token(&rotated)?;
    }
    Ok(access_token)
}

async fn fetch_account(client: &Client, access_token: &str) -> Result<AccountSummary> {
    let response = client
        .get(format!(
            "{GRAPH_BASE}/me?$select=displayName,mail,userPrincipalName"
        ))
        .bearer_auth(access_token)
        .send()
        .await?
        .error_for_status()?;
    let user: GraphUser = response.json().await?;
    Ok(AccountSummary {
        display_name: user.display_name,
        email: user
            .mail
            .or(user.user_principal_name)
            .unwrap_or_else(|| "Microsoft account".into()),
    })
}

async fn graph_request(
    client: &Client,
    method: reqwest::Method,
    url: &str,
    access_token: &str,
) -> Result<reqwest::Response> {
    for attempt in 0..=4 {
        let response = client
            .request(method.clone(), url)
            .bearer_auth(access_token)
            .header("Accept", "application/json")
            .send()
            .await?;
        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }
        if (status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()) && attempt < 4 {
            let delay = response
                .headers()
                .get("Retry-After")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(1_u64 << attempt);
            sleep(Duration::from_secs(delay.min(60))).await;
            continue;
        }
        let body = response.text().await.unwrap_or_default();
        return Err(AppError::Message(format!(
            "Microsoft Graph returned {status}: {body}"
        )));
    }
    Err(AppError::Message(
        "Microsoft Graph retries were exhausted".into(),
    ))
}

pub fn email_prefix_matches(address: &str) -> bool {
    address.trim().to_lowercase().starts_with(FIXED_PREFIX)
}

fn acquire_run_lock() -> Result<File> {
    let path = app_support_dir()?.join("cleanup.lock");
    ensure_parent(&path)?;
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(path)?;
    file.try_lock_exclusive()
        .map_err(|_| AppError::AlreadyRunning)?;
    Ok(file)
}

pub async fn run_cleanup<F>(
    settings: &AppSettings,
    trigger: &str,
    mut emit: F,
) -> Result<CleanupResult>
where
    F: FnMut(CleanupEvent),
{
    settings.validate()?;
    if settings.client_id.is_empty() {
        return Err(AppError::Message(
            "Configure the Microsoft Application Client ID first".into(),
        ));
    }
    let _lock = acquire_run_lock()?;
    emit(CleanupEvent::Started {
        dry_run: settings.dry_run,
        trigger: trigger.to_string(),
    });

    let client = Client::builder().timeout(Duration::from_secs(45)).build()?;
    let access_token = refresh_access_token(&client, settings).await?;
    let mut url = Some(format!(
        "{GRAPH_BASE}/me/mailFolders/junkemail/messages\
         ?$select=id,subject,from,receivedDateTime&$top=100"
    ));
    let mut scanned = 0;
    let mut matches = Vec::new();

    while let Some(page_url) = url.take() {
        let response =
            graph_request(&client, reqwest::Method::GET, &page_url, &access_token).await?;
        let page: GraphPage = response.json().await?;
        for message in page.value {
            scanned += 1;
            let sender = message
                .from
                .map(|from| from.email_address.address)
                .unwrap_or_default();
            if email_prefix_matches(&sender) {
                let record = MatchRecord {
                    id: message.id,
                    sender,
                    subject: message.subject.unwrap_or_else(|| "(no subject)".into()),
                    received_at: message
                        .received_date_time
                        .unwrap_or_else(|| "unknown date".into()),
                };
                emit(CleanupEvent::Matched {
                    record: record.clone(),
                });
                matches.push(record);
            }
        }
        emit(CleanupEvent::ScanProgress { scanned });
        url = page.next_link;
    }

    emit(CleanupEvent::ScanComplete {
        scanned,
        matched: matches.len(),
    });
    let mut deleted = 0;
    let mut failed = 0;
    if !settings.dry_run {
        for record in &matches {
            let endpoint = format!("{GRAPH_BASE}/me/messages/{}", record.id);
            match graph_request(&client, reqwest::Method::DELETE, &endpoint, &access_token).await {
                Ok(_) => deleted += 1,
                Err(_) => failed += 1,
            }
            emit(CleanupEvent::DeleteProgress {
                deleted,
                failed,
                total: matches.len(),
            });
        }
    }

    let result = CleanupResult {
        scanned,
        matched: matches.len(),
        deleted,
        failed,
        dry_run: settings.dry_run,
        matches,
    };
    emit(CleanupEvent::Finished {
        result: result.clone(),
    });
    Ok(result)
}

pub fn append_run_record(record: &RunRecord) -> Result<()> {
    let path = app_support_dir()?.join("history.jsonl");
    ensure_parent(&path)?;
    let mut records = load_run_history(199)?;
    records.push(record.clone());
    let mut file = File::create(path)?;
    for item in records {
        writeln!(file, "{}", serde_json::to_string(&item)?)?;
    }
    Ok(())
}

pub fn load_run_history(limit: usize) -> Result<Vec<RunRecord>> {
    let path = app_support_dir()?.join("history.jsonl");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let mut records: Vec<RunRecord> = fs::read_to_string(path)?
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    if records.len() > limit {
        records = records.split_off(records.len() - limit);
    }
    Ok(records)
}

pub async fn execute_recorded_cleanup<F>(
    settings: &AppSettings,
    trigger: &str,
    emit: F,
) -> Result<CleanupResult>
where
    F: FnMut(CleanupEvent),
{
    let started_at = Utc::now();
    let outcome = run_cleanup(settings, trigger, emit).await;
    let record = match &outcome {
        Ok(result) => RunRecord {
            id: Uuid::new_v4().to_string(),
            trigger: trigger.into(),
            started_at,
            finished_at: Utc::now(),
            status: if result.failed == 0 {
                "success".into()
            } else {
                "partial".into()
            },
            result: Some(result.clone()),
            error: None,
        },
        Err(error) => RunRecord {
            id: Uuid::new_v4().to_string(),
            trigger: trigger.into(),
            started_at,
            finished_at: Utc::now(),
            status: "failed".into(),
            result: None,
            error: Some(error.to_string()),
        },
    };
    let _ = append_run_record(&record);
    outcome
}

fn launch_agent_path() -> Result<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| AppError::Message("Could not locate the home folder".into()))?;
    Ok(home
        .join("Library")
        .join("LaunchAgents")
        .join(format!("{APP_ID}.plist")))
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

pub fn apply_schedule(settings: &AppSettings, helper_path: &Path) -> Result<ScheduleStatus> {
    settings.validate()?;
    if !settings.schedule_enabled {
        return disable_schedule();
    }
    let plist_path = launch_agent_path()?;
    ensure_parent(&plist_path)?;
    let log_dir = app_support_dir()?;
    fs::create_dir_all(&log_dir)?;
    let run_at_load = if settings.run_at_login && settings.run_on_startup {
        "<true/>"
    } else {
        "<false/>"
    };
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{label}</string>
  <key>ProgramArguments</key>
  <array>
    <string>{helper}</string>
    <string>--scheduled</string>
  </array>
  <key>StartCalendarInterval</key>
  <dict>
    <key>Hour</key><integer>{hour}</integer>
    <key>Minute</key><integer>{minute}</integer>
  </dict>
  <key>RunAtLoad</key>{run_at_load}
  <key>ProcessType</key><string>Background</string>
  <key>StandardOutPath</key><string>{stdout}</string>
  <key>StandardErrorPath</key><string>{stderr}</string>
</dict>
</plist>
"#,
        label = APP_ID,
        helper = xml_escape(&helper_path.to_string_lossy()),
        hour = settings.run_hour,
        minute = settings.run_minute,
        stdout = xml_escape(&log_dir.join("helper.log").to_string_lossy()),
        stderr = xml_escape(&log_dir.join("helper-error.log").to_string_lossy()),
    );
    fs::write(&plist_path, plist)?;

    let domain = format!("gui/{}", unsafe { libc::geteuid() });
    let _ = Command::new("launchctl")
        .args(["bootout", &domain, &plist_path.to_string_lossy()])
        .status();
    let status = Command::new("launchctl")
        .args(["bootstrap", &domain, &plist_path.to_string_lossy()])
        .status()?;
    if !status.success() {
        return Err(AppError::Message(
            "macOS could not load the background schedule".into(),
        ));
    }
    Ok(schedule_status(settings))
}

pub fn disable_schedule() -> Result<ScheduleStatus> {
    let plist_path = launch_agent_path()?;
    if plist_path.exists() {
        let domain = format!("gui/{}", unsafe { libc::geteuid() });
        let _ = Command::new("launchctl")
            .args(["bootout", &domain, &plist_path.to_string_lossy()])
            .status();
        fs::remove_file(plist_path)?;
    }
    Ok(ScheduleStatus {
        enabled: false,
        loaded: false,
        label: APP_ID.into(),
        next_run_description: None,
    })
}

pub fn schedule_status(settings: &AppSettings) -> ScheduleStatus {
    let loaded = launch_agent_path()
        .map(|path| path.exists())
        .unwrap_or(false);
    ScheduleStatus {
        enabled: settings.schedule_enabled,
        loaded,
        label: APP_ID.into(),
        next_run_description: settings.schedule_enabled.then(|| {
            format!(
                "Daily at {:02}:{:02} local time",
                settings.run_hour, settings.run_minute
            )
        }),
    }
}

pub fn current_state() -> Result<AppStateView> {
    let settings = load_settings()?;
    let latest_run = load_run_history(1)?.pop();
    Ok(AppStateView {
        signed_in: has_refresh_token(),
        fixed_prefix: FIXED_PREFIX.into(),
        schedule: schedule_status(&settings),
        settings,
        latest_run,
    })
}

pub fn helper_path_from_app_executable() -> Result<PathBuf> {
    let executable = std::env::current_exe()?;
    let parent = executable
        .parent()
        .ok_or_else(|| AppError::Message("Could not locate the app executable".into()))?;
    Ok(parent.join("daily-email-cleanout-helper"))
}

pub fn format_local_time(value: DateTime<Utc>) -> String {
    value
        .with_timezone(&Local)
        .format("%Y-%m-%d %H:%M:%S %Z")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_matching_is_case_insensitive_and_anchored() {
        assert!(email_prefix_matches("info@-----mail.FnopEKGKj0M9cG.com"));
        assert!(email_prefix_matches("INFO@-----mail.example.com"));
        assert!(!email_prefix_matches("sales@-----mail.example.com"));
        assert!(!email_prefix_matches("myinfo@-----mail.example.com"));
        assert!(!email_prefix_matches("info@example.com"));
    }

    #[test]
    fn defaults_are_safe() {
        let settings = AppSettings::default();
        assert!(settings.dry_run);
        assert_eq!(FIXED_PREFIX, "info@-----");
    }

    #[test]
    fn invalid_schedule_is_rejected() {
        let mut settings = AppSettings::default();
        settings.run_hour = 25;
        assert!(settings.validate().is_err());
    }
}
