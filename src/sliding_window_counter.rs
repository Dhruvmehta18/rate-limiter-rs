use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use http_body_util::{Empty, Full};
use http_body_util::{BodyExt, combinators::BoxBody};
use hyper::{Request, Response};
use hyper::{Method, StatusCode};
use hyper::body::{Body, Bytes};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;
use tokio::time::Instant;

struct SlidingWindowCounter {
    limit: u32,
    window_size: Duration,
    counter: Arc<Mutex<HashMap<String, ClientData>>>
}

struct ClientData {
    previous_counter: u32,
    current_counter: u32,
    start_time: Instant
}

impl SlidingWindowCounter {
    fn new(limit: u32, window_size: Duration) -> Self {
        return SlidingWindowCounter {
            limit,
            window_size,
            counter: Arc::new(Mutex::new(HashMap::new()))
        }
    }

    fn align_to_minute_boundary(now: Instant) -> Instant {
        let duration_since_epoch = now.duration_since(Instant::now() - Duration::new(24*60*60, 0));
        let elapsed_secs = duration_since_epoch.as_secs();
        let aligned_secs = (elapsed_secs / 60) * 60;
        Instant::now() - Duration::new(24*60*60, 0) + Duration::from_secs(aligned_secs)
    }

    fn allow_request(&self, client_id: String) -> bool {
        let mut counters = self.counter.lock().unwrap();
        let now = Instant::now();
        let current_window_start = Self::align_to_minute_boundary(now);
        println!("client id - {}", client_id.clone());
        let client_data = counters.entry(client_id.clone()).or_insert(ClientData{
            previous_counter: 0,
            current_counter: 0,
            start_time: current_window_start
        });

        if current_window_start > client_data.start_time + self.window_size {
            // Non-consecutive window, reset previous count
            client_data.previous_counter = 0;
        } else if current_window_start > client_data.start_time {
            // Shift the windows
            client_data.previous_counter = client_data.current_counter;
        }

        if current_window_start > client_data.start_time {
            client_data.current_counter = 0;
            client_data.start_time = current_window_start;
        }

        let elapsed = now.duration_since(client_data.start_time);
        let window_fraction = elapsed.as_secs_f64() / self.window_size.as_secs_f64();
        let weighted_count = (client_data.previous_counter as f64 * (1.0 - window_fraction)) + client_data.current_counter as f64;

        if weighted_count < self.limit as f64 {
            client_data.current_counter += 1;
            true
        } else {
            false
        }
    }
}

// We create some utility functions to make Empty and Full bodies
// fit our broadened Response body type.
fn empty() -> BoxBody<Bytes, hyper::Error> {
    Empty::<Bytes>::new()
        .map_err(|never| match never {})
        .boxed()
}
fn full<T: Into<Bytes>>(chunk: T) -> BoxBody<Bytes, hyper::Error> {
    Full::new(chunk.into())
        .map_err(|never| match never {})
        .boxed()
}

async fn echo(
    req: Request<hyper::body::Incoming>,
    rate_limiter: Arc<SlidingWindowCounter>,
    client_ip: Option<SocketAddr>
) -> Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::Error> {
    let client_ip_str = client_ip
        .map(|addr| addr.to_string())
        .unwrap_or_else(|| {
            req.headers()
                .get("x-forwarded-for")
                .and_then(|header| header.to_str().ok())
                .unwrap_or("unknown")
                .to_string()
        });
    match (req.method(), req.uri().path()) {
        (&Method::GET, "/") => Ok(Response::new(full(
            "Try POSTing data to /echo",
        ))),

        (&Method::GET, "/unlimited")=>Ok(Response::new(full(
            "Unlimited! Let's Go!"
        ))),
        (&Method::GET, "/limited")=> {
            // Implement rate limiting logic here
            {

                if !rate_limiter.allow_request(client_ip_str.to_string()) {
                    let mut resp = Response::new(full("Too many requests"));
                    *resp.status_mut() = StatusCode::TOO_MANY_REQUESTS;
                    return Ok(resp);
                }
                
                return Ok(Response::new(full(
                    "limited! Let's Go!"
                )))
            }
        },
        // Return 404 Not Found for other routes.
        _ => {
            let mut not_found = Response::new(empty());
            *not_found.status_mut() = StatusCode::NOT_FOUND;
            Ok(not_found)
        }
    }
}

pub(crate) async fn sliding_window_counter() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // We create a TcpListener and bind it to 127.0.0.1:3000
    let addr = SocketAddr::from(([0,0,0,0], 3000));
    let listener = TcpListener::bind(addr).await?;

    let token_bucket = Arc::new(SlidingWindowCounter::new(10, Duration::from_secs(60)));
    // We start a loop to continuously accept incoming connections
    loop {
        let (stream, _) = listener.accept().await?;

        // Use an adapter to access something implementing `tokio::io` traits as if they implement
        // `hyper::rt` IO traits.
        let client_ip = match stream.peer_addr() {
            Ok(addr) => Some(addr),
            Err(_) => None,
        };
        let io = TokioIo::new(stream);
        let token_bucket = Arc::clone(&token_bucket);

        // Spawn a tokio task to serve multiple connections concurrently
        tokio::task::spawn(async move {
            // Finally, we bind the incoming connection to our `hello` service
            if let Err(err) = http1::Builder::new()
                .serve_connection(io, service_fn(move |request| echo(request, token_bucket.clone(), client_ip.clone())))
                .await
            {
                eprintln!("Error serving connection: {:?}", err);
            }
        });
    }
}