mod backends;
pub mod firmware;

/// Compatibility path for the versioned telemetry backend.
pub mod backend {
    pub use crate::backends::v1::GoldshellV1;
}

/// Compatibility path for the versioned read API.
pub mod web {
    pub use crate::backends::v1::web::GoldshellWebAPI;
}
