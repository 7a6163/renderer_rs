use headless_chrome::{Browser, LaunchOptions, Tab};
use std::sync::Arc;
use std::ffi::OsStr;
use tokio::sync::{Semaphore, Mutex};
use warp::Filter;
use serde::Deserialize;
use std::collections::VecDeque;
use lru::LruCache;
use tokio::time::{Duration, Instant};
use std::num::NonZeroUsize;
use std::env;
use log::{info, error};

#[tokio::main]
async fn main() {
    env_logger::init();

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

    let cache_maxsize = env::var("CACHE_MAXSIZE")
        .unwrap_or_else(|_| "100".to_string())
        .parse::<usize>()
        .expect("CACHE_MAXSIZE must be a positive integer");

    let cache_size = NonZeroUsize::new(cache_maxsize).expect("CACHE_MAXSIZE must be a non-zero integer");
    let cache = Arc::new(Mutex::new(LruCache::new(cache_size))); // Cache max size from environment variable

    let render_route = warp::path("html")
        .and(warp::query::<RenderQuery>())
        .and(with_browser(browser.clone()))
        .and(with_semaphore(semaphore.clone()))
        .and(with_tab_pool(tab_pool.clone()))
        .and(with_cache(cache.clone()))
        .and_then(render_handler);

    info!("Server running on http://0.0.0.0:8080");
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

fn with_cache(
    cache: Arc<Mutex<LruCache<String, (String, Instant)>>>,
) -> impl Filter<Extract = (Arc<Mutex<LruCache<String, (String, Instant)>>>,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || cache.clone())
}

#[derive(Debug)]
struct CustomError;

impl warp::reject::Reject for CustomError {}

async fn render_handler(
    query: RenderQuery,
    browser: Arc<Browser>,
    semaphore: Arc<Semaphore>,
    tab_pool: Arc<Mutex<VecDeque<Arc<Tab>>>>,
    cache: Arc<Mutex<LruCache<String, (String, Instant)>>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    let _permit = semaphore.acquire().await;

    let cache_ttl_secs = env::var("CACHE_TTL")
        .unwrap_or_else(|_| "60".to_string())
        .parse::<u64>()
        .expect("CACHE_TTL must be a positive integer");
    let cache_ttl = Duration::new(cache_ttl_secs, 0);

    info!("Initial request to {}", query.url);

    let start_time = Instant::now();
    
    let mut cache_guard = cache.lock().await;
    if let Some((content, timestamp)) = cache_guard.get(&query.url) {
        if timestamp.elapsed() < cache_ttl {
            info!("Cache hit for {}", query.url);
            return Ok(warp::reply::html(content.clone()));
        }
    }

    let tab = {
        let mut pool = tab_pool.lock().await;
        if let Some(tab) = pool.pop_front() {
            tab
        } else {
            browser.new_tab().map_err(|e| {
                error!("Failed to create new tab: {:?}", e);
                warp::reject::custom(CustomError)
            })?
        }
    };

    tab.navigate_to(&query.url).map_err(|e| {
        error!("Failed to navigate to URL: {:?}", e);
        warp::reject::custom(CustomError)
    })?;
    tab.wait_until_navigated().map_err(|e| {
        error!("Failed to wait until navigated: {:?}", e);
        warp::reject::custom(CustomError)
    })?;

    let content = tab.get_content().map_err(|e| {
        error!("Failed to get content: {:?}", e);
        warp::reject::custom(CustomError)
    })?;

    {
        let mut pool = tab_pool.lock().await;
        pool.push_back(Arc::clone(&tab));
    }

    cache_guard.put(query.url.clone(), (content.clone(), Instant::now()));

    let duration = start_time.elapsed();
    info!("Got {} in {:?} for {}", content.len(), duration, query.url);

    Ok(warp::reply::html(content))
}