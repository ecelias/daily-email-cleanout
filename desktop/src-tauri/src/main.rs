use daily_email_core::{execute_recorded_cleanup, load_settings};

fn main() {
    if std::env::args().any(|argument| argument == "--headless-cleanup") {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("could not start the cleanup runtime");
        let exit_code = runtime.block_on(async {
            let settings = match load_settings() {
                Ok(settings) => settings,
                Err(error) => {
                    eprintln!("Could not load settings: {error}");
                    return 2;
                }
            };
            match execute_recorded_cleanup(&settings, "scheduled", |event| {
                if let Ok(line) = serde_json::to_string(&event) {
                    println!("{line}");
                }
            })
            .await
            {
                Ok(_) => 0,
                Err(error) => {
                    eprintln!("Cleanup failed: {error}");
                    1
                }
            }
        });
        std::process::exit(exit_code);
    }

    daily_email_cleanout_lib::run();
}
