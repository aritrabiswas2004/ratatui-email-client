/**************************************************************
SPDX License Identifier: GPL-3

Main runtime for the application launch, authentication and run.

Authors: Aritra Biswas <aritrabb@gmail.com>
         Arnav Waghdhare <arnavwaghdhare@gmail.com>
***************************************************************/

use color_eyre::eyre::Result;
use ratatui_email_client::{app::App, auth, gmail::GmailClient, logging};

fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    color_eyre::install()?;

    logging::init("logs/app.log")?;
    logging::info("App launch: ratatui-email-client starting up");

    logging::info("Starting OAuth authentication");
    let session = match auth::authenticate() {
        Ok(session) => {
            logging::info("OAuth authentication succeeded");
            session
        }
        Err(err) => {
            logging::error(&format!("OAuth authentication failed: {err}"));
            return Err(err);
        }
    };

    logging::info("Initializing Gmail client");
    let gmail = match GmailClient::new(session.access_token) {
        Ok(client) => {
            logging::info("Gmail client initialized");
            client
        }
        Err(err) => {
            logging::error(&format!("Gmail client initialization failed: {err}"));
            return Err(err);
        }
    };

    let mut app = App::new(gmail);

    logging::info("Launching terminal UI");
    let run_result = ratatui::run(|terminal| app.run(terminal));
    match run_result {
        Ok(()) => {
            logging::info("Terminal UI exited cleanly");
            Ok(())
        }
        Err(err) => {
            logging::error(&format!("Terminal UI exited with error: {err}"));
            Err(err.into())
        }
    }
}
