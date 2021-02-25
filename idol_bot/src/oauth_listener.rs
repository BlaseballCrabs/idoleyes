use super::db::Database;
use anyhow::Result;
use async_std::prelude::*;
use tide::{prelude::*, Request};

#[derive(Serialize, Clone)]
pub struct OAuth {
    pub redirect_uri: String,
    pub client_id: String,
    pub client_secret: String,
}

#[derive(Clone)]
struct State {
    db: Database,
    oauth: OAuth,
}

pub fn listen(db: &Database, oauth: OAuth) -> impl Future<Output = Result<()>> {
    let db = db.clone();

    async move {
        let state = State { db, oauth };

        let mut app = tide::with_state(state);
        app.at("/").get(auth);
        app.at("/redirect").get(redirect);
        app.listen("0.0.0.0:4130").await?;

        Ok(())
    }
}

#[derive(Deserialize)]
struct AuthCode {
    code: String,
}

#[derive(Serialize)]
struct ExchangeRequest<'a> {
    code: &'a str,
    grant_type: &'static str,
    scope: &'static str,
    #[serde(flatten)]
    oauth: &'a OAuth,
}

#[derive(Deserialize)]
struct WebhookResponse {
    url: String,
}

#[derive(Deserialize)]
struct TokenResponse {
    webhook: WebhookResponse,
}

async fn auth(req: Request<State>) -> tide::Result {
    let state = req.state();
    let mut url = http_types::Url::parse("https://discord.com/api/oauth2/authorize").unwrap();
    url.query_pairs_mut()
        .append_pair("client_id", &state.oauth.client_id)
        .append_pair("redirect_uri", &state.oauth.redirect_uri)
        .append_pair("response_type", "code")
        .append_pair("scope", "webhook.incoming");
    Ok(tide::Redirect::new(url).into())
}

async fn redirect(req: Request<State>) -> tide::Result {
    let AuthCode { code } = req.query()?;
    let state = req.state();

    let exchange = ExchangeRequest {
        code: &code,
        grant_type: "authorization_code",
        scope: "webhook.incoming",
        oauth: &state.oauth,
    };

    let resp: TokenResponse = surf::post("https://discord.com/api/oauth2/token")
        .body(surf::Body::from_form(&exchange)?)
        .recv_json()
        .await?;

    let WebhookResponse { url } = resp.webhook;

    state.db.add_url(&url).await?;

    Ok("Added!".into())
}
