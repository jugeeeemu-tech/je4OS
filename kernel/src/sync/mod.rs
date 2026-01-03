//! 同期プリミティブ
//!
//! このモジュールはブロッキング同期プリミティブを提供します。

pub mod blocking_mutex;
pub mod wait_queue;

pub use blocking_mutex::BlockingMutex;
