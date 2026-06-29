mod controllers;

use askama::Template;
use once_cell::sync::OnceCell;
use spin_sdk::http::{IntoResponse, Router};
use spin_sdk::http_component;

use serde::Serialize;

// ── Embed the MaxMind database into the binary ─────────────────

const MMDB_BYTES: &[u8] =
    include_bytes!("../GeoLite2-City_20260605/GeoLite2-City.mmdb");

static DB_READER: OnceCell<maxminddb::Reader<Vec<u8>>> = OnceCell::new();

// ── Askama templates ───────────────────────────────────────────

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate;

#[derive(Template)]
#[template(path = "result.html")]
pub struct ResultTemplate {
    ip: String,
    city: String,
    country: String,
    country_code: String,
    latitude: String,
    longitude: String,
    timezone: String,
}

#[derive(Template)]
#[template(path = "error.html")]
pub struct ErrorTemplate {
    message: String,
}

// ── Wizer pre-initialization ───────────────────────────────────

#[export_name = "wizer.initialize"]
extern "C" fn pre_init() {
    let reader = maxminddb::Reader::from_source(MMDB_BYTES.to_vec())
        .expect("Wizer: failed to parse MaxMind GeoLite2-City database");
    DB_READER
        .set(reader)
        .ok()
        .expect("Wizer: DB_READER already initialised");
}

// ── Spin HTTP handler ──────────────────────────────────────────

#[http_component]
fn handle_iplookup(req: spin_sdk::http::Request) -> anyhow::Result<impl IntoResponse, anyhow::Error> {
    let mut router = Router::default();
    
    router.get("/", controllers::index::handle_index);
    router.get("/lookup", controllers::lookup::handle_lookup);
    Ok(router.handle(req))
}

// ── Helpers ────────────────────────────────────────────────────



fn name_en<N>(named: &Option<N>) -> String
where
    N: HasNames,
{
    named
        .as_ref()
        .and_then(|n| n.names())
        .and_then(|names| names.get("en"))
        .map(|s| s.to_string())
        .unwrap_or_else(|| "—".to_string())
}

trait HasNames {
    fn names(&self) -> Option<&std::collections::BTreeMap<&str, &str>>;
}

impl HasNames for maxminddb::geoip2::city::City<'_> {
    fn names(&self) -> Option<&std::collections::BTreeMap<&str, &str>> {
        self.names.as_ref()
    }
}

impl HasNames for maxminddb::geoip2::country::Country<'_> {
    fn names(&self) -> Option<&std::collections::BTreeMap<&str, &str>> {
        self.names.as_ref()
    }
}

fn parse_query_param<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    let prefix = format!("{}=", key);
    query.split('&').find_map(|pair| {
        if pair.starts_with(&prefix) {
            Some(&pair[prefix.len()..])
        } else if pair == key {
            Some("")
        } else {
            None
        }
    })
}

// ── JSON types ─────────────────────────────────────────────────

#[derive(Serialize)]
struct JsonResult {
    ip: String,
    city: String,
    country: String,
    country_code: Option<String>,
    latitude: Option<f64>,
    longitude: Option<f64>,
    timezone: Option<String>,
}

#[derive(Serialize)]
struct JsonError {
    error: &'static str,
}
