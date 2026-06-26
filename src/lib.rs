use once_cell::sync::OnceCell;
use spin_sdk::http::{IntoResponse, Response};
use spin_sdk::http_component;
use std::net::IpAddr;
use std::str::FromStr;

use maxminddb::geoip2::City;
use serde::Serialize;

// ── Embed the MaxMind database into the binary ─────────────────

/// The MaxMind GeoLite2-City database bytes, baked into the .wasm at compile time.
/// Wizer will parse this into a `maxminddb::Reader` in linear memory and
/// snapshot the result — runtime lookups never touch the raw bytes again.
const MMDB_BYTES: &[u8] =
    include_bytes!("../GeoLite2-City_20260605/GeoLite2-City.mmdb");

/// The pre-initialized reader, set during Wizer pre-init.
static DB_READER: OnceCell<maxminddb::Reader<Vec<u8>>> = OnceCell::new();

// ── Wizer pre-initialization ───────────────────────────────────

/// Called by the Wizer tool at build time.
/// Wizer instantiates the module, calls this exported function, then snapshots
/// the resulting linear-memory state into the final .wasm binary.
/// At runtime this function is never called again — all data is already in memory.
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
    let path = req
        .header("spin-path-info")
        .and_then(|v| v.as_str())
        .unwrap_or("/");

    // Extract IP from path: strip leading '/' and any trailing garbage
    let ip_str = path.trim_start_matches('/').trim();

    if ip_str.is_empty() {
        return Ok(Response::builder()
            .status(400)
            .header("content-type", "application/json")
            .body(serde_json::to_string(&ErrorResponse {
                error: "no IP address in path — try /8.8.8.8",
            })?)
            .build());
    }

    let ip: IpAddr = IpAddr::from_str(ip_str).map_err(|_| {
        anyhow::anyhow!("invalid IP address: {ip_str}")
    })?;

    let reader = DB_READER
        .get()
        .expect("DB_READER was not initialized — did Wizer pre-init run?");

    match reader.lookup::<City<'_>>(ip) {
        Ok(city) => {
            let country_ref = &city.country;
            let result = LookupResult {
                ip: ip.to_string(),
                city: city
                    .city
                    .as_ref()
                    .and_then(|c| c.names.as_ref())
                    .and_then(|n| n.get("en").map(|s| s.to_string())),
                country: country_ref
                    .as_ref()
                    .and_then(|c| c.names.as_ref())
                    .and_then(|n| n.get("en").map(|s| s.to_string())),
                country_code: country_ref
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

            let body = serde_json::to_string(&result)?;
            Ok(Response::builder()
                .status(200)
                .header("content-type", "application/json")
                .body(body)
                .build())
        }
        Err(maxminddb::MaxMindDBError::AddressNotFoundError(_)) => {
            Ok(Response::builder()
                .status(404)
                .header("content-type", "application/json")
                .body(serde_json::to_string(&ErrorResponse {
                    error: "IP address not found in database",
                })?)
                .build())
        }
        Err(e) => Err(anyhow::anyhow!("MaxMind lookup error: {e}")),
    }
}

// ── Response types ──────────────────────────────────────────────

#[derive(Serialize)]
struct LookupResult {
    ip: String,
    city: Option<String>,
    country: Option<String>,
    country_code: Option<String>,
    latitude: Option<f64>,
    longitude: Option<f64>,
    timezone: Option<String>,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: &'static str,
}
