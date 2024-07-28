mod logger_http;

mod fixed_window;
mod token_bucket;
mod leak_bucket;
mod sliding_window_log;
mod sliding_window_counter;

enum RateLimitingStrategy {
    FixedWindow,
    SlidingWindowLog,
    TokenBucket,
    LeakyBucket,
    SlidingWindowCounter
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
   const STRATEGY: RateLimitingStrategy = RateLimitingStrategy::SlidingWindowCounter;

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
        },
        RateLimitingStrategy::SlidingWindowCounter => {
            use sliding_window_counter::sliding_window_counter;
            return sliding_window_counter().await
        }
        _ => {
            panic!("No such strategy")
        }
    }
}
