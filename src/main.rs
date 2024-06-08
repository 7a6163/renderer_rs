use headless_chrome::{Browser, LaunchOptions};
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::Semaphore;
use warp::Filter;
use std::ffi::OsStr;

#[tokio::main]
async fn main() {
    let args: Vec<&OsStr> = vec![
        OsStr::new("--no-sandbox"),
        OsStr::new("--disable-gpu"),
        OsStr::new("--ignore-certificate-errors"),
    ];

    let browser = Arc::new(
        Browser::new(LaunchOptions {
            headless: true,
            args,
            ..Default::default()
        })
        .expect("Failed to launch browser"),
    );

    let semaphore = Arc::new(Semaphore::new(4));

    let render_route = warp::path("html")
        .and(warp::query::<RenderQuery>())
        .and(with_browser(browser))
        .and(with_semaphore(semaphore))
        .and_then(render_handler);

    warp::serve(render_route).run(([127, 0, 0, 1], 8080)).await;
}

#[derive(Deserialize)]
struct RenderQuery {
    url: String,
}

fn with_browser(
    browser: Arc<Browser>,
) -> impl Filter<Extract = (Arc<Browser>,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || browser.clone())
}

fn with_semaphore(
    semaphore: Arc<Semaphore>,
) -> impl Filter<Extract = (Arc<Semaphore>,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || semaphore.clone())
}

#[derive(Debug)]
struct CustomError;

impl warp::reject::Reject for CustomError {}

async fn render_handler(
    query: RenderQuery,
    browser: Arc<Browser>,
    semaphore: Arc<Semaphore>,
) -> Result<impl warp::Reply, warp::Rejection> {
    let _permit = semaphore.acquire().await;

    let tab = browser.new_tab().map_err(|e| {
        eprintln!("Failed to create new tab: {:?}", e);
        warp::reject::custom(CustomError)
    })?;

    tab.navigate_to(&query.url).map_err(|e| {
        eprintln!("Failed to navigate to URL: {:?}", e);
        warp::reject::custom(CustomError)
    })?;
    tab.wait_until_navigated().map_err(|e| {
        eprintln!("Failed to wait until navigated: {:?}", e);
        warp::reject::custom(CustomError)
    })?;

    let content = tab.get_content().map_err(|e| {
        eprintln!("Failed to get content: {:?}", e);
        warp::reject::custom(CustomError)
    })?;

    Ok(warp::reply::html(content))
}
