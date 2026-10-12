//! 将棋のルール・探索・評価と、エンジン用の対局・学習ツール。

pub(crate) mod eval;
pub(crate) mod game;
pub(crate) mod params;
pub(crate) mod position;
pub(crate) mod search;

#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod arena;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod label;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod nn_train;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod tune;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod usi;

#[cfg(target_arch = "wasm32")]
mod browser_clock;
