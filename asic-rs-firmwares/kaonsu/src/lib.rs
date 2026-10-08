// SPDX-License-Identifier: Apache-2.0
mod backends;
pub mod firmware;
mod telemetry;
#[cfg(test)]
mod tests;
mod web;

/// Compatibility path for the versioned telemetry backend.
pub mod backend {
    pub use crate::backends::v1::KaonsuMiner;
}
