use headless_chrome::{Browser, LaunchOptions, Tab};
use std::sync::Arc;
use std::ffi::OsStr;
use tokio::sync::{Semaphore, Mutex};
use warp::Filter;
use serde::Deserialize;
use std::collections::VecDeque;
use lru::LruCache;
use std::num::NonZeroUsize;
use std::time::{Duration, Instant};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

const CACHE_TTL: Duration = Duration::from_secs(600); // 10 minutes

#[tokio::main]
async fn main() {
    let args: Vec<&OsStr> = vec![
        OsStr::new("--no-sandbox"),
        OsStr::new("--disable-gpu"),
        OsStr::new("--disable-dev-shm-usage"),
        OsStr::new("--headless"),
        OsStr::new("--disable-software-rasterizer"),
        OsStr::new("--no-zygote"),
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
    let cache = Arc::new(Mutex::new(LruCache::new(NonZeroUsize::new(1000).unwrap())));

    let render_route = warp::path("html")
        .and(warp::query::<RenderQuery>())
        .and(with_browser(browser.clone()))
        .and(with_semaphore(semaphore.clone()))
        .and(with_tab_pool(tab_pool.clone()))
        .and(with_cache(cache.clone()))
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

fn with_cache(
    cache: Arc<Mutex<LruCache<u64, (String, Instant)>>>,
) -> impl Filter<Extract = (Arc<Mutex<LruCache<u64, (String, Instant)>>>,), Error = std::convert::Infallible> + Clone {
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
    cache: Arc<Mutex<LruCache<u64, (String, Instant)>>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    let _permit = semaphore.acquire().await;

    let hash = hash_url(&query.url);

    // Check if the response is in the cache and if it's still valid.
    let mut cache_guard = cache.lock().await;
    if let Some((response, timestamp)) = cache_guard.get(&hash) {
        if timestamp.elapsed() < CACHE_TTL {
            return Ok(warp::reply::html(response.clone()));
        }
    }

    // If not, use headless_chrome to get the DOM.
    drop(cache_guard); // release the lock before using headless_chrome
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

    // Wait for a bit to allow SPA content to load.
    tokio::time::sleep(Duration::from_secs(3)).await;

    let content = tab.get_content().map_err(|e| {
        eprintln!("Failed to get content: {:?}", e);
        warp::reject::custom(CustomError)
    })?;

    {
        let mut cache_guard = cache.lock().await;
        cache_guard.put(hash, (content.clone(), Instant::now()));
    }

    {
        let mut pool = tab_pool.lock().await;
        pool.push_back(Arc::clone(&tab));
    }

    Ok(warp::reply::html(content))
}

fn hash_url(url: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    url.hash(&mut hasher);
    hasher.finish()
}
