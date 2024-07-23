use http_body_util::BodyExt;
use hyper::body::Body;

mod logger_http;

mod fixed_window;
mod token_bucket;

enum RateLimitingStrategy {
    FixedWindow,
    SlidingWindow,
    TokenBucket,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
   const STRATEGY: RateLimitingStrategy = RateLimitingStrategy::TokenBucket;

    match STRATEGY {
        RateLimitingStrategy::FixedWindow => {
            use fixed_window::fixed_window;

            return fixed_window().await
        }
        RateLimitingStrategy::SlidingWindow => {
            todo!("Need to implement this")
        }
        RateLimitingStrategy::TokenBucket => {
            use token_bucket::token_bucket;

            return token_bucket().await
        }
    }
}
