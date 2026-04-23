use color_eyre::eyre::Result;
use ratatui_email_client::{
    app::App,
    auth,
    gmail::GmailClient
};

fn main() -> Result<()> {
    color_eyre::install()?;

    let session = auth::authenticate()?;
    let gmail = GmailClient::new(session.access_token);
    let mut app = App::new(gmail);

    ratatui::run(|terminal| app.run(terminal))?;
    Ok(())
}
