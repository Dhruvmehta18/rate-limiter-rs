use http_body_util::BodyExt;
use hyper::body::Body;
use crate::token_bucket::token_bucket;

mod logger_http;

mod fixed_window;
mod token_bucket;
mod leak_bucket;
mod sliding_window_log;

enum RateLimitingStrategy {
    FixedWindow,
    SlidingWindowLog,
    TokenBucket,
    LeakyBucket
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
   const STRATEGY: RateLimitingStrategy = RateLimitingStrategy::SlidingWindowLog;

    match STRATEGY {
        RateLimitingStrategy::FixedWindow => {
            use fixed_window::fixed_window;

            return fixed_window().await
        }
        RateLimitingStrategy::SlidingWindowLog => {
            use sliding_window_log::fixed_window;
            return  fixed_window().await
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
