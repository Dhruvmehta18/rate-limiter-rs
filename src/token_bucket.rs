use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
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

type TokenBucket = Arc<Mutex<HashMap<String, u64>>>;

const SECONDS_PER_TOKEN:u64 = 60/10;
const TOTAL_TOKENS_PER_CLIENT:u64 = 10;

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
    token_bucket: TokenBucket,
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
                let mut buckets = token_bucket.lock().unwrap();
                let count = buckets.entry(client_ip_str.clone()).or_insert(TOTAL_TOKENS_PER_CLIENT);

                if *count <1 {
                    let mut resp = Response::new(full("Too many requests"));
                    *resp.status_mut() = StatusCode::TOO_MANY_REQUESTS;
                    return Ok(resp);
                } else {
                    *count -= 1;

                    return Ok(Response::new(full(
                        format!("limited! Let's Go! Tokens left: {}", *count)
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

pub(crate) async fn token_bucket() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // We create a TcpListener and bind it to 127.0.0.1:3000
    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    let listener = TcpListener::bind(addr).await?;

    let token_bucket = Arc::new(Mutex::new(HashMap::new()));
    {
        let buckets_clone = Arc::clone(&token_bucket);
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(SECONDS_PER_TOKEN));
            loop {
                interval.tick().await;
                let mut buckets_clone = buckets_clone.lock().unwrap();
                for ( client, tokens) in buckets_clone.iter_mut() {
                    println!("{} has {}", client, tokens);
                    if *tokens>=TOTAL_TOKENS_PER_CLIENT {
                        *tokens=TOTAL_TOKENS_PER_CLIENT // can remove the value of the client if the value get greater than TOTAL_TOKENS_PER_CLIENT to optimize the memory, but for POC purpose not doing it.
                    } else {
                        *tokens += 1
                    }

                }
            }
        });
    }
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