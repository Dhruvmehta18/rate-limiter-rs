use http_body_util::BodyExt;
use hyper::body::Body;
use crate::token_bucket::token_bucket;

mod logger_http;

mod fixed_window;
mod token_bucket;
mod leak_bucket;

enum RateLimitingStrategy {
    FixedWindow,
    SlidingWindow,
    TokenBucket,
    LeakyBucket
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
   const STRATEGY: RateLimitingStrategy = RateLimitingStrategy::LeakyBucket;

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
        },
        RateLimitingStrategy::LeakyBucket => {
            use leak_bucket::leak_bucket;
            return leak_bucket().await;
        }
        _ => {
            panic!("No such strategy")
        }
    }
}
