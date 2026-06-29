use daily_email_core::{
    apply_schedule as apply_local_schedule, begin_device_sign_in as begin_sign_in,
    clear_refresh_token, complete_device_sign_in as complete_sign_in, current_state,
    disable_schedule as disable_local_schedule, execute_recorded_cleanup,
    helper_path_from_app_executable, load_run_history, load_settings, save_settings,
    AccountSummary, AppError, AppSettings, AppStateView, CleanupEvent, CleanupResult,
    DeviceCodePrompt, DeviceSession, RunRecord, ScheduleStatus,
};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, State};

struct RuntimeState {
    device_session: Mutex<Option<DeviceSession>>,
}

#[tauri::command]
fn get_app_state() -> Result<AppStateView, AppError> {
    current_state()
}

#[tauri::command]
fn save_app_settings(settings: AppSettings) -> Result<AppStateView, AppError> {
    save_settings(&settings)?;
    current_state()
}

#[tauri::command]
async fn begin_device_sign_in(
    client_id: String,
    state: State<'_, RuntimeState>,
) -> Result<DeviceCodePrompt, AppError> {
    let (prompt, session) = begin_sign_in(&client_id).await?;
    *state
        .device_session
        .lock()
        .map_err(|_| AppError::Message("Sign-in state is unavailable".into()))? = Some(session);
    Ok(prompt)
}

#[tauri::command]
async fn complete_device_sign_in(
    state: State<'_, RuntimeState>,
) -> Result<AccountSummary, AppError> {
    let session = state
        .device_session
        .lock()
        .map_err(|_| AppError::Message("Sign-in state is unavailable".into()))?
        .take()
        .ok_or_else(|| AppError::Message("Start Microsoft sign-in first".into()))?;
    let account = complete_sign_in(session).await?;
    let mut settings = load_settings()?;
    settings.account_email = Some(account.email.clone());
    settings.account_name = Some(account.display_name.clone());
    save_settings(&settings)?;
    Ok(account)
}

#[tauri::command]
fn sign_out() -> Result<AppStateView, AppError> {
    clear_refresh_token()?;
    let mut settings = load_settings()?;
    settings.account_email = None;
    settings.account_name = None;
    save_settings(&settings)?;
    current_state()
}

#[tauri::command]
async fn run_cleanup(app: AppHandle) -> Result<CleanupResult, AppError> {
    let settings = load_settings()?;
    let event_app = app.clone();
    let outcome = execute_recorded_cleanup(&settings, "manual", move |event: CleanupEvent| {
        let _ = event_app.emit("cleanup-progress", event);
    })
    .await;
    if let Err(error) = &outcome {
        let _ = app.emit(
            "cleanup-progress",
            CleanupEvent::Failed {
                message: error.to_string(),
            },
        );
    }
    outcome
}

#[tauri::command]
fn get_run_history() -> Result<Vec<RunRecord>, AppError> {
    let mut history = load_run_history(100)?;
    history.reverse();
    Ok(history)
}

#[tauri::command]
fn apply_schedule() -> Result<ScheduleStatus, AppError> {
    let settings = load_settings()?;
    let helper = helper_path_from_app_executable()?;
    if !helper.exists() {
        return Err(AppError::Message(format!(
            "Background helper was not found at {}. Install and run the packaged app before enabling scheduling.",
            helper.display()
        )));
    }
    apply_local_schedule(&settings, &helper)
}

#[tauri::command]
fn disable_schedule() -> Result<ScheduleStatus, AppError> {
    disable_local_schedule()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(RuntimeState {
            device_session: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            get_app_state,
            save_app_settings,
            begin_device_sign_in,
            complete_device_sign_in,
            sign_out,
            run_cleanup,
            get_run_history,
            apply_schedule,
            disable_schedule
        ])
        .run(tauri::generate_context!())
        .expect("error while running Daily Email Cleanout");
}
