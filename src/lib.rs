use askama::Template;
use once_cell::sync::OnceCell;
use spin_sdk::http::{IntoResponse, Response};
use spin_sdk::http_component;
use std::net::IpAddr;
use std::str::FromStr;

use maxminddb::geoip2::City;
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
struct ResultTemplate {
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
struct ErrorTemplate {
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
fn handle_iplookup(req: spin_sdk::http::Request) -> anyhow::Result<impl IntoResponse> {
    let path: &str = req
        .header("spin-path-info")
        .and_then(|v| v.as_str())
        .unwrap_or("/");

    // ── Route: / → serve the HTML page ──────────────────────
    if path == "/" || path.is_empty() {
        let html = IndexTemplate.render()?;
        return Ok(Response::builder()
            .status(200)
            .header("content-type", "text/html; charset=utf-8")
            .body(html)
            .build());
    }

    // ── Route: /lookup?ip=... → HTML fragment for HTMX ──────
    if path == "/lookup" {
        let query = extract_query(&req);
        let ip_str = parse_query_param(&query, "ip").unwrap_or("");

        if ip_str.is_empty() {
            let html = ErrorTemplate {
                message: "Please enter an IP address.".into(),
            }
            .render()?;
            return Ok(Response::builder()
                .status(200)
                .header("content-type", "text/html; charset=utf-8")
                .body(html)
                .build());
        }

        let ip = match IpAddr::from_str(ip_str) {
            Ok(ip) => ip,
            Err(_) => {
                let html = ErrorTemplate {
                    message: format!("\u{201c}{ip_str}\u{201d} is not a valid IP address."),
                }
                .render()?;
                return Ok(Response::builder()
                    .status(200)
                    .header("content-type", "text/html; charset=utf-8")
                    .body(html)
                    .build());
            }
        };

        let reader = DB_READER
            .get()
            .expect("DB_READER was not initialized");

        match reader.lookup::<City<'_>>(ip) {
            Ok(city) => {
                let html = ResultTemplate {
                    ip: ip.to_string(),
                    city: name_en(&city.city),
                    country: name_en(&city.country),
                    country_code: city
                        .country
                        .as_ref()
                        .and_then(|c| c.iso_code)
                        .unwrap_or("")
                        .to_string(),
                    latitude: city
                        .location
                        .as_ref()
                        .and_then(|l| l.latitude)
                        .map(|v| format!("{:.4}", v))
                        .unwrap_or_default(),
                    longitude: city
                        .location
                        .as_ref()
                        .and_then(|l| l.longitude)
                        .map(|v| format!("{:.4}", v))
                        .unwrap_or_default(),
                    timezone: city
                        .location
                        .as_ref()
                        .and_then(|l| l.time_zone)
                        .unwrap_or("")
                        .to_string(),
                }
                .render()?;
                return Ok(Response::builder()
                    .status(200)
                    .header("content-type", "text/html; charset=utf-8")
                    .body(html)
                    .build());
            }
            Err(maxminddb::MaxMindDBError::AddressNotFoundError(_)) => {
                let html = ErrorTemplate {
                    message: "IP address not found in database.".into(),
                }
                .render()?;
                return Ok(Response::builder()
                    .status(200)
                    .header("content-type", "text/html; charset=utf-8")
                    .body(html)
                    .build());
            }
            Err(e) => {
                let html = ErrorTemplate {
                    message: format!("Lookup error: {e}"),
                }
                .render()?;
                return Ok(Response::builder()
                    .status(200)
                    .header("content-type", "text/html; charset=utf-8")
                    .body(html)
                    .build());
            }
        }
    }

    // ── Route: /{ip} → JSON (existing behaviour) ────────────
    let ip_str = path.trim_start_matches('/').trim();

    if ip_str.is_empty() {
        return Ok(Response::builder()
            .status(400)
            .header("content-type", "application/json")
            .body(serde_json::to_string(&JsonError {
                error: "no IP address in path \u{2014} try /8.8.8.8",
            })?)
            .build());
    }

    let ip = IpAddr::from_str(ip_str)
        .map_err(|_| anyhow::anyhow!("invalid IP address: {ip_str}"))?;

    let reader = DB_READER
        .get()
        .expect("DB_READER was not initialized");

    match reader.lookup::<City<'_>>(ip) {
        Ok(city) => {
            let result = JsonResult {
                ip: ip.to_string(),
                city: name_en(&city.city),
                country: name_en(&city.country),
                country_code: city
                    .country
                    .as_ref()
                    .and_then(|c| c.iso_code)
                    .map(|c| c.to_string()),
                latitude: city.location.as_ref().and_then(|l| l.latitude),
                longitude: city.location.as_ref().and_then(|l| l.longitude),
                timezone: city
                    .location
                    .as_ref()
                    .and_then(|l| l.time_zone)
                    .map(|t| t.to_string()),
            };
            Ok(Response::builder()
                .status(200)
                .header("content-type", "application/json")
                .body(serde_json::to_string(&result)?)
                .build())
        }
        Err(maxminddb::MaxMindDBError::AddressNotFoundError(_)) => Ok(Response::builder()
            .status(404)
            .header("content-type", "application/json")
            .body(serde_json::to_string(&JsonError {
                error: "IP address not found in database",
            })?)
            .build()),
        Err(e) => Err(anyhow::anyhow!("MaxMind lookup error: {e}")),
    }
}

// ── Helpers ────────────────────────────────────────────────────

/// Extract the query string from the `spin-full-url` header
/// (e.g. "http://localhost:3000/lookup?ip=8.8.8.8" -> "ip=8.8.8.8").
fn extract_query(req: &spin_sdk::http::Request) -> String {
    let full_url: &str = req
        .header("spin-full-url")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    full_url
        .split('?')
        .nth(1)
        .unwrap_or("")
        .to_string()
}

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
