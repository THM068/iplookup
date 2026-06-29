use crate::{name_en, parse_query_param, ErrorTemplate, ResultTemplate, DB_READER};
use askama::Template;
use maxminddb::geoip2::City;
use spin_sdk::http::{IntoResponse, Params, Request, Response};
use std::str::FromStr;
use std::net::IpAddr;

pub fn handle_lookup(
    req: Request,
    params: Params,
) -> anyhow::Result<impl IntoResponse, anyhow::Error> {
    println!("Params: {:?} /lookup", params);
    // ── Route: /lookup?ip=... → HTML fragment for HTMX ──────
    let query = extract_query(&req);
    let ip_str = parse_query_param(&query, "ip").unwrap_or("");

    if ip_str.is_empty() {
        let html = ErrorTemplate {
            message: "Please enter an IP address.".into(),
        }.render()?;
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