use headless_chrome::{Browser, LaunchOptions, Tab};
use std::sync::Arc;
use std::ffi::OsStr;
use tokio::sync::{Semaphore, Mutex};
use warp::Filter;
use serde::Deserialize;
use std::collections::VecDeque;

#[tokio::main]
async fn main() {
    let args: Vec<&OsStr> = vec![
        OsStr::new("--no-sandbox"),
        OsStr::new("--disable-gpu"),
        OsStr::new("--disable-dev-shm-usage"),
        OsStr::new("--headless"),
        OsStr::new("--disable-software-rasterizer"),
        OsStr::new("--no-zygote"),
        OsStr::new("--single-process"),
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
    let tab_pool = Arc::new(Mutex::new(VecDeque::new()));

    let render_route = warp::path("html")
        .and(warp::query::<RenderQuery>())
        .and(with_browser(browser.clone()))
        .and(with_semaphore(semaphore.clone()))
        .and(with_tab_pool(tab_pool.clone()))
        .and_then(render_handler);

    println!("Server running on http://0.0.0.0:8080");
    warp::serve(render_route).run(([0, 0, 0, 0], 8080)).await;
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

fn with_tab_pool(
    tab_pool: Arc<Mutex<VecDeque<Arc<Tab>>>>,
) -> impl Filter<Extract = (Arc<Mutex<VecDeque<Arc<Tab>>>>,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || tab_pool.clone())
}

#[derive(Debug)]
struct CustomError;

impl warp::reject::Reject for CustomError {}

async fn render_handler(
    query: RenderQuery,
    browser: Arc<Browser>,
    semaphore: Arc<Semaphore>,
    tab_pool: Arc<Mutex<VecDeque<Arc<Tab>>>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    let _permit = semaphore.acquire().await;

    let tab = {
        let mut pool = tab_pool.lock().await;
        if let Some(tab) = pool.pop_front() {
            tab
        } else {
            browser.new_tab().map_err(|e| {
                eprintln!("Failed to create new tab: {:?}", e);
                warp::reject::custom(CustomError)
            })?
        }
    };

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

    {
        let mut pool = tab_pool.lock().await;
        pool.push_back(Arc::clone(&tab));
    }

    Ok(warp::reply::html(content))
}