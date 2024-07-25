use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use http_body_util::{Empty, Full};
use hyper::body::{Body, Bytes};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;
use hyper::{Method, StatusCode};
use http_body_util::{combinators::BoxBody, BodyExt};
use tokio::time::{Instant, interval};

struct SlidingWindow {
    limit: u32,
    window_size: Duration,
    counter: Arc<Mutex<HashMap<String, (u32, Instant)>>>
}

impl SlidingWindow {
    fn new(limit: u32, window_size: Duration) -> Self {
        return SlidingWindow {
            limit,
            window_size,
            counter: Arc::new(Mutex::new(HashMap::new()))
        }
    }

    fn allow_request(&self, client_id: String) -> bool {
        let mut counters = self.counter.lock().unwrap();
        let current_time = Instant::now();
        println!("client id - {}", client_id.clone());
        let entry = counters.entry(client_id.clone()).or_insert((0, current_time));

        if current_time.duration_since(entry.1) > self.window_size {
            entry.0 = 0;
            entry.1 = current_time
        }

        if entry.0<self.limit {
            entry.0+=1;
            println!("counter {}", entry.0);
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
    rate_limiter: Arc<SlidingWindow>,
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

                if !rate_limiter.allow_request(client_ip.unwrap().to_string()) {
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

pub(crate) async fn fixed_window() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // We create a TcpListener and bind it to 127.0.0.1:3000
    let addr = SocketAddr::from(([0,0,0,0], 3000));
    let listener = TcpListener::bind(addr).await?;

    let token_bucket = Arc::new(SlidingWindow::new(10, Duration::from_secs(60)));
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