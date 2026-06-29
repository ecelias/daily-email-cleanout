use std::path::PathBuf;
use std::process::Command;

fn main() {
    let executable = std::env::current_exe().expect("could not locate the helper executable");
    let app_executable: PathBuf = executable
        .parent()
        .expect("could not locate the application bundle")
        .join("daily-email-cleanout");

    let status = Command::new(app_executable)
        .arg("--headless-cleanup")
        .status()
        .expect("could not start the headless cleanup");

    std::process::exit(status.code().unwrap_or(1));
}
