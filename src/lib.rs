//! GeoVeil CN0 Analysis Library
//!
//! High-performance GNSS CN0 analysis with:
//! - RINEX observation file parsing
//! - Navigation file parsing (BRDC)
//! - TLE parsing and simplified SGP4 (fallback when no BRDC)
//! - Satellite position computation
//! - Satellite visibility prediction (expected vs observed)
//! - CN0 statistics and timeseries
//! - Anomaly detection (interference, jamming, spoofing indicators)
//! - JSON export with proper string keys
//!
//! Author: Miluta Dulea-Flueras
//! Date: January 2026

pub mod types;
pub mod rinex;
pub mod navigation;
pub mod tle;
pub mod visibility;
pub mod cn0;
pub mod python;

pub use types::*;
pub use rinex::*;
pub use navigation::*;
pub use tle::*;
pub use visibility::*;
pub use cn0::*;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

// Physical constants
pub const SPEED_OF_LIGHT: f64 = 299792458.0;
pub const GM_WGS84: f64 = 3.986005e14;
pub const OMEGA_EARTH: f64 = 7.2921151467e-5;

// GLONASS constants (PZ-90)
pub const GM_GLO: f64 = 3.9860044e14;
pub const RE_GLO: f64 = 6378136.0;
pub const J2_GLO: f64 = 1.0826257e-3;
pub const OMEGA_GLO: f64 = 7.292115e-5;
