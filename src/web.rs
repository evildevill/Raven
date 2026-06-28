use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::sse::{Event, Sse};
use axum::response::{Html, IntoResponse};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde::Serialize;
use tokio::sync::{broadcast, RwLock};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use tower_http::cors::CorsLayer;
use tracing::{error, info};
use uuid::Uuid;

use crate::client;
use crate::engine::{self, SearchUpdate};
use crate::filter;
use crate::manifest::Manifest;
use crate::rate_limiter::RateLimiter;
use crate::types;

#[derive(Clone)]
struct SearchSession {
    tx: broadcast::Sender<SearchUpdate>,
    #[allow(dead_code)]
    results: Arc<RwLock<Vec<types::QueryResult>>>,
    complete: Arc<RwLock<Option<types::SearchResults>>>,
    #[allow(dead_code)]
    shutdown: Arc<AtomicBool>,
    #[allow(dead_code)]
    created_at: tokio::time::Instant,
}

#[derive(Clone)]
struct AppState {
    sessions: Arc<RwLock<HashMap<String, SearchSession>>>,
}

#[derive(Deserialize)]
struct SearchParams {
    usernames: String,
    #[serde(default = "default_concurrency")]
    concurrency: usize,
    #[serde(default = "default_timeout")]
    timeout: f64,
    #[serde(default = "default_retry")]
    retry: usize,
    #[serde(default)]
    nsfw: bool,
    #[serde(default)]
    local: bool,
    #[serde(default)]
    tor: bool,
    #[serde(default)]
    unique_tor: bool,
    rate_limit: Option<f64>,
    proxy: Option<String>,
    site: Option<String>,
    tag: Option<String>,
    #[serde(default)]
    ignore_exclusions: bool,
    #[serde(default)]
    profile: bool,
    #[serde(default)]
    deep: bool,
    #[serde(default)]
    avatar_match: bool,
    #[serde(default)]
    #[allow(dead_code)]
    variants: bool,
}

fn default_concurrency() -> usize { 200 }
fn default_timeout() -> f64 { 20.0 }
fn default_retry() -> usize { 1 }

#[derive(Serialize)]
struct ApiResponse {
    session_id: String,
}

#[derive(Deserialize)]
struct ExportQuery {
    filter: Option<String>,
}

pub fn router() -> Router {
    let state = AppState {
        sessions: Arc::new(RwLock::new(HashMap::new())),
    };

    Router::new()
        .route("/", get(index_handler))
        .route("/search", post(search_handler))
        .route("/stream/:id", get(stream_handler))
        .route("/export/:id/:format", get(export_handler))
        .route("/results/:id", get(results_handler))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

async fn index_handler() -> Html<&'static str> {
    Html(INDEX_HTML)
}

async fn search_handler(
    State(state): State<AppState>,
    Json(params): Json<SearchParams>,
) -> Result<Json<ApiResponse>, (StatusCode, String)> {
    let session_id = Uuid::new_v4().to_string();
    let (tx, _) = broadcast::channel(4096);
    let shutdown = Arc::new(AtomicBool::new(false));
    let results = Arc::new(RwLock::new(Vec::new()));
    let complete = Arc::new(RwLock::new(None));

    let session = SearchSession {
        tx: tx.clone(),
        results: results.clone(),
        complete: complete.clone(),
        shutdown: shutdown.clone(),
        created_at: tokio::time::Instant::now(),
    };

    state.sessions.write().await.insert(session_id.clone(), session);

    let state_clone = state.clone();
    let sid = session_id.clone();
    let tx_clone = tx.clone();
    let params_clone = params.usernames.clone();

    tokio::spawn(async move {
        let rx = tx_clone.subscribe();
        let collector = tokio::spawn(async move {
            let mut stream = BroadcastStream::new(rx);
            while let Some(msg) = stream.next().await {
                match msg {
                    Ok(SearchUpdate::Result(r)) => {
                        results.write().await.push(r);
                    }
                    Ok(SearchUpdate::Complete { total, claimed, available, unknown, illegal, waf }) => {
                        let finalized = types::SearchResults {
                            username: params_clone.clone(),
                            timestamp: chrono::Utc::now().to_rfc3339(),
                            total_sites: total,
                            claimed_count: claimed,
                            available_count: available,
                            unknown_count: unknown,
                            illegal_count: illegal,
                            waf_count: waf,
                            results: results.read().await.clone(),
                        };
                        *complete.write().await = Some(finalized);
                    }
                    _ => {}
                }
            }
        });

        if let Err(e) = run_search(params, tx_clone, shutdown.clone()).await {
            error!("Search session {sid} failed: {e}");
            shutdown.store(true, Ordering::Relaxed);
        }
        collector.await.ok();

        tokio::time::sleep(Duration::from_secs(120)).await;
        state_clone.sessions.write().await.remove(&sid);
        info!("Cleaned up session {sid}");
    });

    Ok(Json(ApiResponse { session_id }))
}

async fn run_search(
    params: SearchParams,
    tx: broadcast::Sender<SearchUpdate>,
    shutdown: Arc<AtomicBool>,
) -> Result<(), crate::error::RavenError> {
    let proxy_url = if params.tor || params.unique_tor {
        Some("socks5://127.0.0.1:9050".to_string())
    } else {
        params.proxy.clone()
    };

    let http_client = client::create_http_client(params.timeout, proxy_url.as_deref())?;

    let manifest = Manifest::load_default(&http_client, params.local).await?;
    info!("Loaded {} sites from manifest", manifest.len());

    let exclusions: std::collections::HashSet<String> = if params.ignore_exclusions {
        std::collections::HashSet::new()
    } else {
        filter::load_exclusions(&http_client).await
    };

    let site_list: Vec<String> = params.site
        .as_ref()
        .map(|s| s.split(',').map(|x| x.trim().to_string()).collect())
        .unwrap_or_default();

    let tag_list: Vec<String> = params.tag
        .as_ref()
        .map(|t| t.split(',').map(|x| x.trim().to_string()).collect())
        .unwrap_or_default();

    let sites = filter::filter_sites(
        manifest.sites,
        params.nsfw,
        &site_list,
        &exclusions,
        &tag_list,
    );

    if sites.is_empty() {
        tx.send(SearchUpdate::Error("No sites to search after filtering".to_string())).ok();
        return Ok(());
    }

    let rate_limiter = params.rate_limit.map(RateLimiter::new);

    let usernames: Vec<String> = params.usernames
        .split(|c: char| c == ',' || c == '\n')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    if usernames.is_empty() {
        tx.send(SearchUpdate::Error("No usernames provided".to_string())).ok();
        return Ok(());
    }

    for username in &usernames {
        if shutdown.load(Ordering::Relaxed) {
            break;
        }
        engine::search_username_stream(
            username,
            &sites,
            &http_client,
            params.concurrency,
            params.retry,
            rate_limiter.clone(),
            params.unique_tor,
            shutdown.clone(),
            tx.clone(),
        )
        .await;
    }

    // Intelligence pipeline (post-processing)
    if (params.profile || params.deep || params.avatar_match) && !shutdown.load(Ordering::Relaxed) {
        if let Some(_username) = usernames.first() {
            // Full web intelligence requires collecting results first
            // For now, send a placeholder to indicate intelligence is available
            tx.send(SearchUpdate::Intelligence {
                confidence: 0.0,
                profile_count: 0,
                graph_json: String::new(),
                timeline_json: String::new(),
                domain_json: String::new(),
            }).ok();
        }
    }

    Ok(())
}

async fn stream_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Sse<impl tokio_stream::Stream<Item = Result<Event, String>>>, (StatusCode, String)> {
    let session = state
        .sessions
        .read()
        .await
        .get(&id)
        .cloned()
        .ok_or_else(|| (StatusCode::NOT_FOUND, "Session not found".to_string()))?;

    let rx = session.tx.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|msg| {
        match msg {
            Ok(SearchUpdate::Result(r)) => {
                let json = serde_json::to_string(&r).unwrap_or_default();
                Some(Ok(Event::default().event("result").data(json)))
            }
            Ok(SearchUpdate::Progress { completed, total }) => {
                let data = serde_json::json!({"completed": completed, "total": total});
                Some(Ok(Event::default().event("progress").data(data.to_string())))
            }
            Ok(SearchUpdate::Complete { total, claimed, available, unknown, illegal, waf }) => {
                let data = serde_json::json!({
                    "total": total, "claimed": claimed, "available": available,
                    "unknown": unknown, "illegal": illegal, "waf": waf
                });
                Some(Ok(Event::default().event("complete").data(data.to_string())))
            }
            Ok(SearchUpdate::Intelligence { confidence, profile_count, graph_json, timeline_json, domain_json }) => {
                let data = serde_json::json!({
                    "confidence": confidence,
                    "profile_count": profile_count,
                    "graph_json": graph_json,
                    "timeline_json": timeline_json,
                    "domain_json": domain_json,
                });
                Some(Ok(Event::default().event("intelligence").data(data.to_string())))
            }
            Ok(SearchUpdate::Error(e)) => {
                Some(Ok(Event::default().event("error").data(e)))
            }
            Err(_) => None,
        }
    });

    Ok(Sse::new(stream))
}

async fn results_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let session = state
        .sessions
        .read()
        .await
        .get(&id)
        .cloned()
        .ok_or_else(|| (StatusCode::NOT_FOUND, "Session not found".to_string()))?;

    let complete = session.complete.read().await;
    match complete.as_ref() {
        Some(sr) => Ok(Json(serde_json::to_value(sr).unwrap_or_default())),
        None => Err((StatusCode::NOT_FOUND, "Search not complete yet".to_string())),
    }
}

async fn export_handler(
    State(state): State<AppState>,
    Path((id, format)): Path<(String, String)>,
    Query(query): Query<ExportQuery>,
) -> Result<Response, (StatusCode, String)> {
    let session = state
        .sessions
        .read()
        .await
        .get(&id)
        .cloned()
        .ok_or_else(|| (StatusCode::NOT_FOUND, "Session not found".to_string()))?;

    let complete = session.complete.read().await;
    let sr = complete.as_ref().ok_or_else(|| (StatusCode::NOT_FOUND, "Search not complete yet".to_string()))?;

    let filter_claimed = query.filter.as_deref() == Some("claimed");

    let filtered_results: Vec<&types::QueryResult> = if filter_claimed {
        sr.results.iter().filter(|r| r.status == types::QueryStatus::Claimed).collect()
    } else {
        sr.results.iter().collect()
    };

    match format.as_str() {
        "csv" => {
            let mut csv = String::from("username,site_name,url_user,probe_url,status,http_status,response_time_ms,context\n");
            for r in &filtered_results {
                csv.push_str(&format!(
                    "{},{},{},{},{},{},{},{}\n",
                    r.username,
                    r.site_name,
                    r.site_url_user,
                    r.probe_url,
                    r.status,
                    r.http_status.map(|s| s.to_string()).unwrap_or_default(),
                    r.query_time_ms.map(|t| t.to_string()).unwrap_or_default(),
                    r.context.as_deref().unwrap_or(""),
                ));
            }
            Ok(Response::csv(csv))
        }
        "json" => {
            let json = serde_json::to_string_pretty(&sr).unwrap_or_default();
            Ok(Response::json(json))
        }
        "txt" => {
            let mut txt = String::new();
            for r in &filtered_results {
                if !r.site_url_user.is_empty() {
                    txt.push_str(&r.site_url_user);
                    txt.push('\n');
                }
            }
            Ok(Response::txt(txt))
        }
        _ => Err((StatusCode::BAD_REQUEST, "Unsupported format. Use csv, json, or txt".to_string())),
    }
}

struct Response {
    body: String,
    mime: &'static str,
    filename: String,
}

impl Response {
    fn csv(body: String) -> Self {
        Response { body, mime: "text/csv", filename: "results.csv".to_string() }
    }
    fn json(body: String) -> Self {
        Response { body, mime: "application/json", filename: "results.json".to_string() }
    }
    fn txt(body: String) -> Self {
        Response { body, mime: "text/plain", filename: "results.txt".to_string() }
    }
}

impl IntoResponse for Response {
    fn into_response(self) -> axum::response::Response {
        let disposition = format!("attachment; filename=\"{}\"", self.filename);
        ([(header::CONTENT_TYPE, self.mime), (header::CONTENT_DISPOSITION, &disposition)], self.body).into_response()
    }
}

const INDEX_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>Raven — OSINT Username Search</title>
<script src="https://cdn.tailwindcss.com"></script>
<script src="https://cdn.jsdelivr.net/npm/chart.js@4.4.7/dist/chart.umd.min.js"></script>
<script>
tailwind.config = {
  theme: {
    extend: {
      colors: {
        dark: { 950: '#05050a', 900: '#0a0a0f', 850: '#0d0d15', 800: '#0f0f1a', 750: '#12121e', 700: '#161625', 650: '#1a1a2e', 600: '#1c1c30', 550: '#222238', 500: '#2a2a40', 450: '#32324a' },
        emerald: { 400: '#34d399', 500: '#10b981', 600: '#059669' },
      },
      fontFamily: { sans: ['Inter', 'system-ui', 'sans-serif'], mono: ['JetBrains Mono', 'Fira Code', 'monospace'] },
      animation: { 'fade-in': 'fadeIn 0.4s ease-out', 'slide-up': 'slideUp 0.3s ease-out', 'pulse-slow': 'pulse 3s infinite', 'shimmer': 'shimmer 2s infinite linear' },
      keyframes: { fadeIn: { '0%': { opacity: '0' }, '100%': { opacity: '1' } }, slideUp: { '0%': { opacity: '0', transform: 'translateY(8px)' }, '100%': { opacity: '1', transform: 'translateY(0)' } }, shimmer: { '0%': { backgroundPosition: '-200% 0' }, '100%': { backgroundPosition: '200% 0' } } },
    }
  }
}
</script>
<style>
@import url('https://fonts.googleapis.com/css2?family=Inter:wght@300;400;500;600;700;800&family=JetBrains+Mono:wght@400;500;600;700&display=swap');
* { box-sizing: border-box; }
body { background: #05050a; color: #e2e8f0; font-family: 'Inter', system-ui, sans-serif; min-height: 100vh; }
::selection { background: #10b98133; color: #34d399; }
.glass { background: rgba(15,15,26,0.7); backdrop-filter: blur(12px); -webkit-backdrop-filter: blur(12px); border: 1px solid rgba(42,42,64,0.4); }
.glass-hover:hover { border-color: rgba(52,211,153,0.3); }
.shimmer { background: linear-gradient(90deg, transparent 0%, rgba(52,211,153,0.03) 50%, transparent 100%); background-size: 200% 100%; }
@keyframes dotPulse { 0%,80%,100% { transform: scale(0.6); opacity: 0.3; } 40% { transform: scale(1); opacity: 1; } }
.dot-pulse { animation: dotPulse 1.4s infinite ease-in-out both; }
.dot-pulse:nth-child(2) { animation-delay: 0.16s; }
.dot-pulse:nth-child(3) { animation-delay: 0.32s; }
.line-clamp { display: -webkit-box; -webkit-line-clamp: 1; -webkit-box-orient: vertical; overflow: hidden; }
.detail-panel { display: none; }
.detail-panel.open { display: block; }
.result-row { cursor: pointer; }
</style>
</head>
<body class="antialiased">

<div class="max-w-5xl mx-auto px-4 sm:px-6 lg:px-8 py-6 sm:py-10">

  <header class="text-center mb-10 sm:mb-14 animate-fade-in">
    <div class="inline-flex items-center justify-center w-14 h-14 rounded-2xl bg-emerald-500/10 border border-emerald-500/20 mb-5">
      <svg class="w-7 h-7 text-emerald-400" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
        <path stroke-linecap="round" stroke-linejoin="round" d="M21 21l-5.197-5.197m0 0A7.5 7.5 0 105.196 5.196a7.5 7.5 0 0010.607 10.607z" />
      </svg>
    </div>
    <h1 class="text-4xl sm:text-5xl font-extrabold tracking-tight mb-2">
      <span class="text-emerald-400">raven</span>
      <span class="text-gray-600 font-light mx-1">/</span>
      <span class="text-gray-300 font-light">search</span>
    </h1>
    <p class="text-sm text-gray-500 font-light tracking-wide">OSINT username reconnaissance across 400+ social networks</p>
  </header>

  <div class="glass rounded-2xl p-5 sm:p-7 mb-8 animate-slide-up">
    <div class="flex flex-col sm:flex-row gap-3">
      <div class="relative flex-1">
        <textarea
          id="username-input"
          rows="1"
          placeholder="Enter username (comma or newline separated for batch)"
          class="w-full bg-dark-800 border border-dark-500 rounded-xl px-4 py-3.5 text-gray-200 placeholder-gray-600 text-sm outline-none transition-all focus:border-emerald-500/40 focus:ring-2 focus:ring-emerald-500/10 resize-none overflow-hidden"
          autocomplete="off" autocorrect="off" autocapitalize="off" spellcheck="false"
          oninput="autoResize(this)"
        ></textarea>
      </div>
      <button
        id="search-btn"
        class="bg-emerald-600 hover:bg-emerald-500 text-white font-semibold px-8 py-3.5 rounded-xl text-sm transition-all disabled:opacity-30 disabled:cursor-not-allowed active:scale-[0.98] shrink-0"
        onclick="startSearch()"
      >
        Search
      </button>
    </div>

    <div class="mt-4">
      <button onclick="toggleAdvanced()" class="text-xs text-gray-500 hover:text-gray-300 transition-colors flex items-center gap-2 group">
        <svg id="adv-chevron" class="w-3.5 h-3.5 transition-transform duration-200" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M19 9l-7 7-7-7"/></svg>
        <span class="group-hover:text-gray-300">Advanced options</span>
      </button>
      <div id="advanced-options" class="hidden mt-4 grid grid-cols-2 sm:grid-cols-4 gap-4">
        <div>
          <label class="text-[10px] uppercase tracking-widest text-gray-500 block mb-1.5 font-medium">Concurrency</label>
          <input id="opt-concurrency" type="number" value="200" min="1" max="1000" class="w-full bg-dark-800 border border-dark-500 rounded-lg px-3 py-2 text-xs text-gray-300 outline-none focus:border-emerald-500/30" />
        </div>
        <div>
          <label class="text-[10px] uppercase tracking-widest text-gray-500 block mb-1.5 font-medium">Timeout (s)</label>
          <input id="opt-timeout" type="number" value="20" min="1" max="120" step="1" class="w-full bg-dark-800 border border-dark-500 rounded-lg px-3 py-2 text-xs text-gray-300 outline-none focus:border-emerald-500/30" />
        </div>
        <div>
          <label class="text-[10px] uppercase tracking-widest text-gray-500 block mb-1.5 font-medium">Retries</label>
          <input id="opt-retry" type="number" value="1" min="0" max="10" class="w-full bg-dark-800 border border-dark-500 rounded-lg px-3 py-2 text-xs text-gray-300 outline-none focus:border-emerald-500/30" />
        </div>
        <div>
          <label class="text-[10px] uppercase tracking-widest text-gray-500 block mb-1.5 font-medium">Rate /s</label>
          <input id="opt-rate-limit" type="number" min="1" max="1000" placeholder="unlimited" class="w-full bg-dark-800 border border-dark-500 rounded-lg px-3 py-2 text-xs text-gray-300 outline-none focus:border-emerald-500/30" />
        </div>
        <div><label class="flex items-center gap-2.5 text-xs text-gray-400 cursor-pointer mt-1"><input id="opt-nsfw" type="checkbox" class="accent-emerald-500 rounded" /> NSFW</label></div>
        <div><label class="flex items-center gap-2.5 text-xs text-gray-400 cursor-pointer mt-1"><input id="opt-tor" type="checkbox" class="accent-emerald-500 rounded" /> Tor</label></div>
        <div><label class="flex items-center gap-2.5 text-xs text-gray-400 cursor-pointer mt-1"><input id="opt-unique-tor" type="checkbox" class="accent-emerald-500 rounded" /> Unique Tor</label></div>
        <div><label class="flex items-center gap-2.5 text-xs text-gray-400 cursor-pointer mt-1"><input id="opt-ignore-exclusions" type="checkbox" class="accent-emerald-500 rounded" /> No exclusions</label></div>
      </div>
    </div>
  </div>

  <div id="status-bar" class="hidden mb-6 glass rounded-xl p-5 animate-slide-up">
    <div class="flex items-center justify-between mb-3">
      <div class="flex items-center gap-3">
        <div class="flex gap-1">
          <span class="dot-pulse w-2 h-2 rounded-full bg-emerald-400 inline-block"></span>
          <span class="dot-pulse w-2 h-2 rounded-full bg-emerald-400 inline-block"></span>
          <span class="dot-pulse w-2 h-2 rounded-full bg-emerald-400 inline-block"></span>
        </div>
        <span class="text-sm font-medium text-emerald-400" id="status-text">Searching</span>
        <span class="text-xs text-gray-600" id="username-label"></span>
      </div>
      <span class="text-xs text-gray-500 font-mono" id="progress-text">0 / 0</span>
    </div>
    <div class="w-full h-1.5 bg-dark-600 rounded-full overflow-hidden">
      <div id="progress-bar" class="h-full bg-gradient-to-r from-emerald-600 to-emerald-400 rounded-full transition-all duration-500 ease-out" style="width: 0%"></div>
    </div>
  </div>

  <div id="stats" class="hidden mb-6 grid grid-cols-5 gap-3 animate-slide-up">
    <div class="glass rounded-xl py-3 text-center glass-hover transition-all" data-stat="claimed">
      <div class="text-xl sm:text-2xl font-bold text-emerald-400" id="stat-claimed">0</div>
      <div class="text-[10px] uppercase tracking-widest text-gray-500 font-medium mt-0.5">Claimed</div>
    </div>
    <div class="glass rounded-xl py-3 text-center glass-hover transition-all" data-stat="available">
      <div class="text-xl sm:text-2xl font-bold text-gray-400" id="stat-available">0</div>
      <div class="text-[10px] uppercase tracking-widest text-gray-500 font-medium mt-0.5">Available</div>
    </div>
    <div class="glass rounded-xl py-3 text-center glass-hover transition-all" data-stat="unknown">
      <div class="text-xl sm:text-2xl font-bold text-gray-500" id="stat-unknown">0</div>
      <div class="text-[10px] uppercase tracking-widest text-gray-500 font-medium mt-0.5">Unknown</div>
    </div>
    <div class="glass rounded-xl py-3 text-center glass-hover transition-all" data-stat="waf">
      <div class="text-xl sm:text-2xl font-bold text-orange-400" id="stat-waf">0</div>
      <div class="text-[10px] uppercase tracking-widest text-gray-500 font-medium mt.0-5">WAF</div>
    </div>
    <div class="glass rounded-xl py-3 text-center glass-hover transition-all" data-stat="illegal">
      <div class="text-xl sm:text-2xl font-bold text-red-400" id="stat-illegal">0</div>
      <div class="text-[10px] uppercase tracking-widest text-gray-500 font-medium mt-0.5">Illegal</div>
    </div>
  </div>

  <div id="charts" class="hidden mb-6 grid grid-cols-1 sm:grid-cols-2 gap-4 animate-slide-up">
    <div class="glass rounded-xl p-4">
      <canvas id="pie-chart"></canvas>
    </div>
    <div class="glass rounded-xl p-4">
      <canvas id="bar-chart"></canvas>
    </div>
  </div>

  <div id="export-bar" class="hidden mb-6 flex flex-wrap items-center gap-3 animate-slide-up">
    <span class="text-xs text-gray-500 font-medium uppercase tracking-wider">Export</span>
    <button onclick="exportResults('csv')" class="text-xs px-4 py-2 rounded-lg bg-dark-700 border border-dark-500 text-gray-300 hover:border-emerald-500/30 hover:text-emerald-400 transition-all">CSV</button>
    <button onclick="exportResults('json')" class="text-xs px-4 py-2 rounded-lg bg-dark-700 border border-dark-500 text-gray-300 hover:border-emerald-500/30 hover:text-emerald-400 transition-all">JSON</button>
    <button onclick="exportResults('txt')" class="text-xs px-4 py-2 rounded-lg bg-dark-700 border border-dark-500 text-gray-300 hover:border-emerald-500/30 hover:text-emerald-400 transition-all">TXT URLs</button>
    <div class="flex-1"></div>
    <label class="flex items-center gap-2 text-xs text-gray-500 cursor-pointer">
      <span>Claimed only</span>
      <input id="filter-claimed" type="checkbox" checked class="accent-emerald-500 rounded" onchange="applyFilters()" />
    </label>
  </div>

  <div id="filter-sort-bar" class="hidden mb-4 flex flex-wrap items-center gap-3 animate-slide-up">
    <div class="flex items-center gap-2 text-xs text-gray-500">
      <span>Status:</span>
      <label class="flex items-center gap-1 cursor-pointer"><input type="checkbox" class="accent-emerald-500 rounded" value="Claimed" checked onchange="applyFilters()" /><span class="text-emerald-400">Claimed</span></label>
      <label class="flex items-center gap-1 cursor-pointer"><input type="checkbox" class="accent-gray-500 rounded" value="Available" checked onchange="applyFilters()" /><span class="text-gray-400">Available</span></label>
      <label class="flex items-center gap-1 cursor-pointer"><input type="checkbox" class="accent-yellow-500 rounded" value="Unknown" checked onchange="applyFilters()" /><span class="text-yellow-500">Unknown</span></label>
      <label class="flex items-center gap-1 cursor-pointer"><input type="checkbox" class="accent-orange-500 rounded" value="Waf" checked onchange="applyFilters()" /><span class="text-orange-400">WAF</span></label>
      <label class="flex items-center gap-1 cursor-pointer"><input type="checkbox" class="accent-red-500 rounded" value="Illegal" checked onchange="applyFilters()" /><span class="text-red-400">Illegal</span></label>
    </div>
    <div class="flex-1"></div>
    <select id="sort-select" onchange="applySort()" class="text-xs bg-dark-700 border border-dark-500 rounded-lg px-3 py-1.5 text-gray-300 outline-none focus:border-emerald-500/30">
      <option value="name">Sort: Site name</option>
      <option value="time-desc">Sort: Slowest first</option>
      <option value="time-asc">Sort: Fastest first</option>
      <option value="status">Sort: Status</option>
      <option value="http">Sort: HTTP status</option>
    </select>
    <input
      id="search-filter"
      type="text"
      placeholder="Filter results..."
      class="text-xs bg-dark-700 border border-dark-500 rounded-lg px-3 py-1.5 text-gray-300 placeholder-gray-600 outline-none focus:border-emerald-500/30 w-40"
      oninput="applyFilters()"
    />
  </div>

  <div id="results" class="space-y-1"></div>

  <div id="empty-state" class="text-center py-24 animate-fade-in">
    <div class="inline-flex items-center justify-center w-16 h-16 rounded-2xl bg-dark-700 border border-dark-500 mb-5">
      <svg class="w-8 h-8 text-gray-600" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
        <path stroke-linecap="round" stroke-linejoin="round" d="M21 21l-5.197-5.197m0 0A7.5 7.5 0 105.196 5.196a7.5 7.5 0 0010.607 10.607z" />
      </svg>
    </div>
    <p class="text-sm text-gray-600 font-light">Enter a username to begin searching</p>
  </div>

  <div id="loading-placeholder" class="hidden space-y-1.5">
    <div class="h-12 shimmer rounded-lg"></div>
    <div class="h-12 shimmer rounded-lg"></div>
    <div class="h-12 shimmer rounded-lg"></div>
    <div class="h-12 shimmer rounded-lg"></div>
  </div>

</div>

<script>
let eventSource = null;
let sessionId = null;
let allResults = [];
let resultsContainer = document.getElementById('results');
let emptyState = document.getElementById('empty-state');
let statusBar = document.getElementById('status-bar');
let progressBar = document.getElementById('progress-bar');
let progressText = document.getElementById('progress-text');
let statusText = document.getElementById('status-text');
let usernameLabel = document.getElementById('username-label');
let stats = document.getElementById('stats');
let charts = document.getElementById('charts');
let exportBar = document.getElementById('export-bar');
let filterSortBar = document.getElementById('filter-sort-bar');
let searchBtn = document.getElementById('search-btn');
let usernameInput = document.getElementById('username-input');
let loadingPlaceholder = document.getElementById('loading-placeholder');

let claimedCount = 0, availableCount = 0, unknownCount = 0, wafCount = 0, illegalCount = 0;
let pieChartInstance = null;
let barChartInstance = null;

usernameInput.addEventListener('keydown', function(e) {
  if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); startSearch(); }
});

function autoResize(el) {
  el.style.height = 'auto';
  el.style.height = el.scrollHeight + 'px';
}

function toggleAdvanced() {
  const el = document.getElementById('advanced-options');
  const ch = document.getElementById('adv-chevron');
  el.classList.toggle('hidden');
  ch.style.transform = el.classList.contains('hidden') ? 'rotate(0deg)' : 'rotate(180deg)';
}

function startSearch() {
  const val = usernameInput.value.trim();
  if (!val) return;

  if (eventSource) { eventSource.close(); eventSource = null; }

  allResults = [];
  claimedCount = 0; availableCount = 0; unknownCount = 0; wafCount = 0; illegalCount = 0;
  updateStats();
  destroyCharts();
  resultsContainer.innerHTML = '';
  emptyState.classList.add('hidden');
  loadingPlaceholder.classList.remove('hidden');
  statusBar.classList.remove('hidden');
  stats.classList.remove('hidden');
  charts.classList.add('hidden');
  exportBar.classList.add('hidden');
  filterSortBar.classList.add('hidden');
  searchBtn.disabled = true;
  searchBtn.innerHTML = '<svg class="animate-spin w-4 h-4 inline mr-1.5" viewBox="0 0 24 24" fill="none"><circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"/><path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z"/></svg> Search';
  statusText.textContent = 'Starting search...';
  usernameLabel.textContent = '/' + val.split(/[\n,]/)[0].trim();
  progressBar.style.width = '0%';
  progressText.textContent = '0 / 0';

  const params = {
    usernames: val,
    concurrency: parseInt(document.getElementById('opt-concurrency').value) || 200,
    timeout: parseFloat(document.getElementById('opt-timeout').value) || 20,
    retry: parseInt(document.getElementById('opt-retry').value) || 1,
    nsfw: document.getElementById('opt-nsfw').checked,
    tor: document.getElementById('opt-tor').checked,
    unique_tor: document.getElementById('opt-unique-tor').checked,
    rate_limit: parseFloat(document.getElementById('opt-rate-limit').value) || null,
    ignore_exclusions: document.getElementById('opt-ignore-exclusions').checked,
  };

  fetch('/search', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(params),
  })
  .then(r => r.json())
  .then(data => {
    sessionId = data.session_id;
    statusText.textContent = 'Searching';
    connectStream(data.session_id);
  })
  .catch(err => {
    statusText.textContent = 'Connection failed';
    searchBtn.disabled = false;
    searchBtn.innerHTML = 'Search';
    loadingPlaceholder.classList.add('hidden');
  });
}

function connectStream(id) {
  eventSource = new EventSource('/stream/' + id);

  eventSource.addEventListener('result', function(e) {
    const r = JSON.parse(e.data);
    allResults.push(r);
    addResultRow(r);
    switch(r.status) {
      case 'Claimed': claimedCount++; break;
      case 'Available': availableCount++; break;
      case 'Unknown': unknownCount++; break;
      case 'Waf': wafCount++; break;
      case 'Illegal': illegalCount++; break;
    }
    updateStats();
  });

  eventSource.addEventListener('progress', function(e) {
    const p = JSON.parse(e.data);
    const pct = p.total > 0 ? Math.round((p.completed / p.total) * 100) : 0;
    progressBar.style.width = pct + '%';
    progressText.textContent = p.completed + ' / ' + p.total;
  });

  eventSource.addEventListener('complete', function(e) {
    statusText.textContent = 'Search complete';
    progressBar.style.width = '100%';
    searchBtn.disabled = false;
    searchBtn.innerHTML = 'Search';
    loadingPlaceholder.classList.add('hidden');
    eventSource.close();
    eventSource = null;
    exportBar.classList.remove('hidden');
    filterSortBar.classList.remove('hidden');
    renderCharts();
  });

  eventSource.addEventListener('error', function(e) {
    statusText.textContent = 'Error during search';
    searchBtn.disabled = false;
    searchBtn.innerHTML = 'Search';
    loadingPlaceholder.classList.add('hidden');
    eventSource.close();
    eventSource = null;
  });
}

function toggleResultRow(el) {
  const detail = el.nextElementSibling;
  if (detail && detail.classList.contains('detail-panel')) {
    detail.classList.toggle('open');
  }
}

function addResultRow(r) {
  loadingPlaceholder.classList.add('hidden');
  emptyState.classList.add('hidden');

  const statusColors = {
    Claimed: 'bg-emerald-500/15 text-emerald-400 border-emerald-500/25',
    Available: 'bg-gray-500/15 text-gray-400 border-gray-500/25',
    Unknown: 'bg-yellow-500/10 text-yellow-500 border-yellow-500/20',
    Waf: 'bg-orange-500/15 text-orange-400 border-orange-500/25',
    Illegal: 'bg-red-500/15 text-red-400 border-red-500/25',
  };
  const badgeClass = statusColors[r.status] || statusColors.Unknown;
  const timeStr = r.query_time_ms != null ? r.query_time_ms + 'ms' : '—';
  const httpStr = r.http_status != null ? r.http_status : '—';
  const contextStr = r.context || '';
  const probeUrl = r.probe_url || '—';

  const row = document.createElement('div');
  row.className = 'result-row glass rounded-xl px-4 py-3 text-xs transition-all duration-200 hover:border-dark-450';
  row.style.animation = 'slideUp 0.25s ease-out';
  row.id = 'row-' + allResults.length;
  row.onclick = function() { toggleResultRow(row); };
  row.innerHTML =
    '<div class="flex items-center gap-3">' +
      '<span class="text-gray-600 shrink-0 w-2 text-center text-xs">›</span>' +
      '<span class="text-gray-200 font-medium w-36 shrink-0 truncate text-xs" title="' + r.site_name + '">' + r.site_name + '</span>' +
      '<a href="' + r.site_url_user + '" target="_blank" class="text-gray-500 hover:text-emerald-400 truncate flex-1 text-xs transition-colors line-clamp" onclick="event.stopPropagation()">' + (r.site_url_user || '—') + '</a>' +
      '<span class="text-gray-600 w-12 text-right shrink-0 font-mono text-[11px]">' + httpStr + '</span>' +
      '<span class="text-gray-600 w-16 text-right shrink-0 font-mono text-[11px]">' + timeStr + '</span>' +
      '<span class="px-2.5 py-1 rounded-lg text-[11px] font-medium border shrink-0 ' + badgeClass + '">' + r.status + '</span>' +
    '</div>' +
    '<div class="detail-panel mt-3 pt-3 border-t border-dark-500">' +
      '<div class="grid grid-cols-2 sm:grid-cols-3 gap-2 text-[11px]">' +
        '<div><span class="text-gray-600">Username:</span> <span class="text-gray-300">' + (r.username || '—') + '</span></div>' +
        '<div><span class="text-gray-600">Profile URL:</span> <a href="' + r.site_url_user + '" target="_blank" class="text-emerald-400 hover:text-emerald-300" onclick="event.stopPropagation()">' + (r.site_url_user || '—') + '</a></div>' +
        '<div><span class="text-gray-600">Probe URL:</span> <span class="text-gray-400 break-all">' + probeUrl + '</span></div>' +
        '<div><span class="text-gray-600">HTTP Status:</span> <span class="text-gray-300">' + httpStr + '</span></div>' +
        '<div><span class="text-gray-600">Response Time:</span> <span class="text-gray-300">' + timeStr + '</span></div>' +
        '<div><span class="text-gray-600">Status:</span> <span class="' + badgeClass.split(' ').slice(0,2).join(' ') + '">' + r.status + '</span></div>' +
        (contextStr ? '<div class="col-span-full"><span class="text-gray-600">Context:</span> <span class="text-gray-400">' + contextStr + '</span></div>' : '') +
      '</div>' +
    '</div>';

  resultsContainer.appendChild(row);
}

function applyFilters() {
  const searchTerm = document.getElementById('search-filter').value.toLowerCase();
  const claimedOnly = document.getElementById('filter-claimed').checked;
  const checkedStatuses = new Set();
  document.querySelectorAll('#filter-sort-bar input[type="checkbox"]').forEach(cb => {
    if (cb.checked) checkedStatuses.add(cb.value);
  });

  const rows = resultsContainer.querySelectorAll('.result-row');
  rows.forEach(row => {
    const detail = row.querySelector('.detail-panel');
    const html = row.innerHTML;
    const statusMatch = html.match(/>([A-Za-z]+)<\/span><\/div><div class="detail-panel/);
    const status = statusMatch ? statusMatch[1] : '';
    const siteName = (html.match(/title="([^"]+)"/) || ['',''])[1].toLowerCase();
    const urlMatch = html.match(/href="([^"]+)"/);
    const url = urlMatch ? urlMatch[1].toLowerCase() : '';
    const textContent = row.textContent.toLowerCase();

    const matchesSearch = !searchTerm || textContent.includes(searchTerm);
    const matchesStatus = checkedStatuses.has(status);
    const matchesClaimed = !claimedOnly || status === 'Claimed';

    if (matchesSearch && matchesStatus && matchesClaimed) {
      row.style.display = '';
    } else {
      row.style.display = 'none';
    }
  });
}

function applySort() {
  const sortBy = document.getElementById('sort-select').value;
  const rows = Array.from(resultsContainer.querySelectorAll('.result-row'));

  rows.sort((a, b) => {
    const htmlA = a.innerHTML;
    const htmlB = b.innerHTML;
    const nameA = (htmlA.match(/title="([^"]+)"/) || ['',''])[1];
    const nameB = (htmlB.match(/title="([^"]+)"/) || ['',''])[1];
    const getStatus = (h) => (h.match(/>([A-Za-z]+)<\/span><\/div><div class="detail-panel/) || ['',''])[1];
    const getHttp = (h) => parseInt((h.match(/<span class="text-gray-600 w-12[^>]*>(\d+)<\/span>/) || [,'0'])[1]) || 0;
    const getTime = (h) => parseInt((h.match(/<span class="text-gray-600 w-16[^>]*>(\d+)ms/) || [,'0'])[1]) || 0;

    switch(sortBy) {
      case 'name': return nameA.localeCompare(nameB);
      case 'time-desc': return getTime(htmlB) - getTime(htmlA);
      case 'time-asc': return getTime(htmlA) - getTime(htmlB);
      case 'status': {
        const order = { Claimed: 0, Available: 1, Unknown: 2, Waf: 3, Illegal: 4 };
        return (order[getStatus(htmlA)] || 99) - (order[getStatus(htmlB)] || 99);
      }
      case 'http': return getHttp(htmlA) - getHttp(htmlB);
      default: return 0;
    }
  });

  rows.forEach(row => resultsContainer.appendChild(row));
}

function renderCharts() {
  charts.classList.remove('hidden');
  setTimeout(() => {
    // Pie chart
    const pieCtx = document.getElementById('pie-chart').getContext('2d');
    if (pieChartInstance) pieChartInstance.destroy();
    pieChartInstance = new Chart(pieCtx, {
      type: 'doughnut',
      data: {
        labels: ['Claimed', 'Available', 'Unknown', 'WAF', 'Illegal'],
        datasets: [{
          data: [claimedCount, availableCount, unknownCount, wafCount, illegalCount],
          backgroundColor: ['#10b981', '#6b7280', '#eab308', '#f97316', '#ef4444'],
          borderColor: '#0f0f1a',
          borderWidth: 2,
        }]
      },
      options: {
        responsive: true,
        plugins: {
          legend: {
            position: 'bottom',
            labels: { color: '#9ca3af', font: { size: 11, family: 'Inter' }, padding: 12, usePointStyle: true }
          },
          title: {
            display: true,
            text: 'Status Distribution',
            color: '#e2e8f0',
            font: { size: 13, family: 'Inter', weight: '600' },
            padding: { bottom: 12 }
          }
        }
      }
    });

    // Bar chart: top 20 by response time
    const withTime = allResults.filter(r => r.query_time_ms != null).sort((a, b) => b.query_time_ms - a.query_time_ms).slice(0, 20);
    const barCtx = document.getElementById('bar-chart').getContext('2d');
    if (barChartInstance) barChartInstance.destroy();
    barChartInstance = new Chart(barCtx, {
      type: 'bar',
      data: {
        labels: withTime.map(r => r.site_name),
        datasets: [{
          label: 'Response time (ms)',
          data: withTime.map(r => r.query_time_ms),
          backgroundColor: withTime.map(r => r.status === 'Claimed' ? '#10b98166' : '#6b728066'),
          borderColor: withTime.map(r => r.status === 'Claimed' ? '#10b981' : '#6b7280'),
          borderWidth: 1,
          borderRadius: 3,
        }]
      },
      options: {
        responsive: true,
        indexAxis: 'y',
        plugins: {
          legend: { display: false },
          title: {
            display: true,
            text: 'Response Times (top 20)',
            color: '#e2e8f0',
            font: { size: 13, family: 'Inter', weight: '600' },
            padding: { bottom: 12 }
          }
        },
        scales: {
          x: { grid: { color: '#1c1c30' }, ticks: { color: '#6b7280', font: { size: 10 } } },
          y: { grid: { display: false }, ticks: { color: '#9ca3af', font: { size: 10 } } }
        }
      }
    });
  }, 100);
}

function destroyCharts() {
  if (pieChartInstance) { pieChartInstance.destroy(); pieChartInstance = null; }
  if (barChartInstance) { barChartInstance.destroy(); barChartInstance = null; }
}

function updateStats() {
  document.getElementById('stat-claimed').textContent = claimedCount;
  document.getElementById('stat-available').textContent = availableCount;
  document.getElementById('stat-unknown').textContent = unknownCount;
  document.getElementById('stat-waf').textContent = wafCount;
  document.getElementById('stat-illegal').textContent = illegalCount;
}

function exportResults(format) {
  if (!sessionId) return;
  const filterParam = document.getElementById('filter-claimed').checked ? '?filter=claimed' : '';
  window.location.href = '/export/' + sessionId + '/' + format + filterParam;
}
</script>
</body>
</html>"##;
