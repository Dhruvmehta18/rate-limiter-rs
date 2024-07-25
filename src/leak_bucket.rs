use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use http_body_util::{Empty, Full};
use http_body_util::{BodyExt, combinators::BoxBody};
use hyper::{Request, Response};
use hyper::{Method, StatusCode};
use hyper::body::{Body, Bytes};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;
use tokio::time::interval;

struct AtomicBucket {
    capacity: usize,
    current_level: AtomicUsize,
    leak_rate: usize,
}

impl AtomicBucket {
    fn new(capacity: usize, leak_rate: usize) -> Self {
        Self {
            capacity,
            current_level: AtomicUsize::new(0),
            leak_rate,
        }
    }

    fn add_request(&self) -> bool {
        let mut current_level = self.current_level.load(Ordering::Relaxed);
        loop {
            if current_level >= self.capacity {
                return false;
            }
            // Try to increment the current level atomically
            match self.current_level.compare_exchange_weak(
                current_level,
                current_level + 1,
                Ordering::SeqCst,
                Ordering::Relaxed,
            ) {
                Ok(_) => return true,
                Err(updated_level) => current_level = updated_level,
            }
        }
    }

    fn leak(&self) {
        let current_level = self.current_level.load(Ordering::Relaxed);
        println!("Leaking starting currently at {}", current_level);
        if current_level > 0 {
            // Try to decrement the current level atomically
            self.current_level.fetch_sub(self.leak_rate, Ordering::Relaxed);
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
    bucket: Arc<AtomicBucket>
) -> Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::Error> {
    match (req.method(), req.uri().path()) {
        (&Method::GET, "/") => Ok(Response::new(full(
            "Try POSTing data to /echo",
        ))),

        (&Method::GET, "/unlimited")=>Ok(Response::new(full(
            "Unlimited! Let's Go!"
        ))),
        (&Method::GET, "/limited")=> {
            // Implementing rate limiting logic here
            {
                return if bucket.add_request() {
                    let mut resp = Response::new(full("Too many requests"));
                    *resp.status_mut() = StatusCode::TOO_MANY_REQUESTS;
                    Ok(resp)
                } else {
                    Ok(Response::new(full(
                        "limited! Let's Go!".to_string()
                    )))
                }
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

pub(crate) async fn leak_bucket() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    let listener = TcpListener::bind(addr).await?;
    let bucket = Arc::new(AtomicBucket::new(10, 1));
    {
        let buckets_clone = Arc::clone(&bucket);
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(1));
            loop {
                interval.tick().await;
                buckets_clone.leak()
            }
        });
    }
    // We start a loop to continuously accept incoming connections
    loop {
        let (stream, _) = listener.accept().await?;
        let io = TokioIo::new(stream);
        let token_bucket = Arc::clone(&bucket);

        // Spawn a tokio task to serve multiple connections concurrently
        tokio::task::spawn(async move {
            if let Err(err) = http1::Builder::new()
                .serve_connection(io, service_fn(move |request| echo(request, token_bucket.clone())))
                .await
            {
                eprintln!("Error serving connection: {:?}", err);
            }
        });
    }
}