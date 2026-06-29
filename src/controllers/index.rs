use askama::Template;
use spin_sdk::http::{IntoResponse, Params, Request, Response};
use crate::IndexTemplate;

pub fn handle_index(
    _: Request,
    params: Params,
) -> anyhow::Result<impl IntoResponse, anyhow::Error> {
    println!("Params: {:?} /index", params);
    let html = IndexTemplate.render()?;
    Ok(Response::builder()
        .status(200)
        .header("content-type", "text/html; charset=utf-8")
        .body(html)
        .build())
}