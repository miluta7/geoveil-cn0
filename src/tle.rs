//! TLE (Two-Line Element) Parser and Simplified SGP4 Predictor
//!
//! Fallback satellite prediction when BRDC navigation is unavailable.
//! Uses simplified SGP4 for GNSS satellites (circular orbits, ~20,200km altitude).
//!
//! TLE Sources (no auth required):
//! - CelesTrak: https://celestrak.org/NORAD/elements/gp.php?GROUP=gnss&FORMAT=tle
//!
//! Author: Miluta Dulea-Flueras
//! Date: January 2026

use crate::types::{Epoch, Ecef};
use std::collections::HashMap;
use std::f64::consts::PI;

// WGS84 constants
const GM: f64 = 3.986004418e14;  // m³/s²
const EARTH_RADIUS: f64 = 6378137.0;  // m
const J2: f64 = 1.08263e-3;
const OMEGA_EARTH: f64 = 7.2921151467e-5;  // rad/s

/// TLE data for one satellite
#[derive(Debug, Clone)]
pub struct TleData {
    pub name: String,
    pub satellite_id: String,  // e.g., "G01", "E11"
    pub norad_id: u32,
    pub epoch_year: i32,
    pub epoch_day: f64,
    pub mean_motion: f64,      // rev/day
    pub eccentricity: f64,
    pub inclination: f64,      // degrees
    pub raan: f64,             // Right Ascension of Ascending Node (degrees)
    pub arg_perigee: f64,      // Argument of Perigee (degrees)
    pub mean_anomaly: f64,     // degrees
    pub bstar: f64,            // Drag term
}

impl TleData {
    /// Convert TLE epoch to Epoch struct
    pub fn get_epoch(&self) -> Epoch {
        let year = if self.epoch_year < 57 {
            2000 + self.epoch_year
        } else {
            1900 + self.epoch_year
        };
        
        let day_of_year = self.epoch_day.floor() as u32;
        let fraction = self.epoch_day - day_of_year as f64;
        
        // Convert DOY to month/day
        let (month, day) = doy_to_md(year, day_of_year);
        
        let total_seconds = fraction * 86400.0;
        let hour = (total_seconds / 3600.0).floor() as u32;
        let minute = ((total_seconds - hour as f64 * 3600.0) / 60.0).floor() as u32;
        let second = total_seconds - hour as f64 * 3600.0 - minute as f64 * 60.0;
        
        Epoch::new(year, month, day, hour, minute, second)
    }
}

fn doy_to_md(year: i32, doy: u32) -> (u32, u32) {
    let is_leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
    let days_in_month = if is_leap {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    
    let mut remaining = doy;
    for (i, &days) in days_in_month.iter().enumerate() {
        if remaining <= days {
            return ((i + 1) as u32, remaining);
        }
        remaining -= days;
    }
    (12, 31)
}

/// Parse TLE file content
pub fn parse_tle(content: &str) -> Vec<TleData> {
    let lines: Vec<&str> = content.lines().collect();
    let mut tles = Vec::new();
    
    let mut i = 0;
    while i + 2 < lines.len() {
        let line0 = lines[i].trim();
        let line1 = lines[i + 1].trim();
        let line2 = lines[i + 2].trim();
        
        // Check if this is a valid TLE set
        if !line1.starts_with('1') || !line2.starts_with('2') {
            i += 1;
            continue;
        }
        
        // Parse name line (line 0)
        let name = line0.to_string();
        let satellite_id = extract_gnss_id(&name);
        
        // Parse line 1
        let norad_id: u32 = line1[2..7].trim().parse().unwrap_or(0);
        let epoch_year: i32 = line1[18..20].trim().parse().unwrap_or(0);
        let epoch_day: f64 = line1[20..32].trim().parse().unwrap_or(0.0);
        
        // BSTAR drag term (with implicit decimal point)
        let bstar_str = &line1[53..61];
        let bstar = parse_tle_decimal(bstar_str);
        
        // Parse line 2
        let inclination: f64 = line2[8..16].trim().parse().unwrap_or(0.0);
        let raan: f64 = line2[17..25].trim().parse().unwrap_or(0.0);
        
        // Eccentricity has implicit leading decimal
        let ecc_str = format!("0.{}", &line2[26..33].trim());
        let eccentricity: f64 = ecc_str.parse().unwrap_or(0.0);
        
        let arg_perigee: f64 = line2[34..42].trim().parse().unwrap_or(0.0);
        let mean_anomaly: f64 = line2[43..51].trim().parse().unwrap_or(0.0);
        let mean_motion: f64 = line2[52..63].trim().parse().unwrap_or(0.0);
        
        if !satellite_id.is_empty() && mean_motion > 0.0 {
            tles.push(TleData {
                name,
                satellite_id,
                norad_id,
                epoch_year,
                epoch_day,
                mean_motion,
                eccentricity,
                inclination,
                raan,
                arg_perigee,
                mean_anomaly,
                bstar,
            });
        }
        
        i += 3;
    }
    
    tles
}

/// Parse TLE decimal format (e.g., " 12345-4" -> 0.12345e-4)
fn parse_tle_decimal(s: &str) -> f64 {
    let s = s.trim();
    if s.is_empty() {
        return 0.0;
    }
    
    // Format: " 12345-4" means 0.12345 × 10^-4
    let mut mantissa_str = String::new();
    let mut exp_str = String::new();
    let mut in_exp = false;
    let mut sign = 1.0f64;
    
    for (i, c) in s.chars().enumerate() {
        if c == '-' && i == 0 {
            sign = -1.0;
        } else if c == '-' || c == '+' {
            in_exp = true;
            if c == '-' {
                exp_str.push(c);
            }
        } else if c.is_digit(10) {
            if in_exp {
                exp_str.push(c);
            } else {
                mantissa_str.push(c);
            }
        }
    }
    
    if mantissa_str.is_empty() {
        return 0.0;
    }
    
    let mantissa: f64 = format!("0.{}", mantissa_str).parse().unwrap_or(0.0);
    let exp: i32 = exp_str.parse().unwrap_or(0);
    
    sign * mantissa * 10.0f64.powi(exp)
}

/// Extract GNSS satellite ID from TLE name
fn extract_gnss_id(name: &str) -> String {
    let name_upper = name.to_uppercase();
    
    // GPS: "GPS BIIR-2  (PRN 13)" -> "G13"
    // GPS: "NAVSTAR 43 (USA 132)" with NORAD mapping
    if name_upper.contains("GPS") || name_upper.contains("NAVSTAR") {
        if let Some(prn_start) = name_upper.find("PRN") {
            let after_prn = &name_upper[prn_start + 3..];
            let prn: String = after_prn.chars()
                .skip_while(|c| !c.is_digit(10))
                .take_while(|c| c.is_digit(10))
                .collect();
            if let Ok(num) = prn.parse::<u32>() {
                return format!("G{:02}", num);
            }
        }
    }
    
    // GLONASS: "COSMOS 2527 (GLONASS-M)" with slot mapping
    // GLONASS: "GLONASS-M 751" 
    if name_upper.contains("GLONASS") || name_upper.contains("COSMOS 2") {
        // Would need GLONASS slot database for accurate PRN
        // For now, use sequential numbering
        return String::new();  // Skip GLONASS in TLE fallback
    }
    
    // Galileo: "GSAT0101 (GALILEO 1)" -> "E01"
    // Galileo: "GALILEO-FM2 (E14)"
    if name_upper.contains("GALILEO") || name_upper.contains("GSAT") {
        // Look for E## pattern
        if let Some(e_pos) = name_upper.find("(E") {
            let after_e = &name_upper[e_pos + 2..];
            let num: String = after_e.chars()
                .take_while(|c| c.is_digit(10))
                .collect();
            if let Ok(prn) = num.parse::<u32>() {
                return format!("E{:02}", prn);
            }
        }
        // Try GSAT number
        if let Some(gsat_pos) = name_upper.find("GSAT") {
            let after_gsat = &name_upper[gsat_pos + 4..];
            let num: String = after_gsat.chars()
                .take_while(|c| c.is_digit(10))
                .collect();
            if let Ok(n) = num.parse::<u32>() {
                // GSAT numbers don't directly map to PRN
                return format!("E{:02}", n % 36 + 1);
            }
        }
    }
    
    // BeiDou: "BEIDOU-3 M1" or "BEIDOU 3"
    if name_upper.contains("BEIDOU") {
        // Would need BeiDou PRN database
        return String::new();  // Skip BeiDou in TLE fallback
    }
    
    // QZSS: "QZS-1 (MICHIBIKI)"
    if name_upper.contains("QZS") || name_upper.contains("MICHIBIKI") {
        let num: String = name_upper.chars()
            .skip_while(|c| !c.is_digit(10))
            .take_while(|c| c.is_digit(10))
            .collect();
        if let Ok(prn) = num.parse::<u32>() {
            return format!("J{:02}", prn);
        }
    }
    
    String::new()
}

/// Simplified SGP4 propagator for GNSS (near-circular MEO orbits)
#[derive(Debug, Clone)]
pub struct SimplifiedSgp4 {
    tles: HashMap<String, TleData>,
}

impl SimplifiedSgp4 {
    pub fn new(tles: Vec<TleData>) -> Self {
        let mut map = HashMap::new();
        for tle in tles {
            if !tle.satellite_id.is_empty() {
                map.insert(tle.satellite_id.clone(), tle);
            }
        }
        Self { tles: map }
    }
    
    pub fn from_tle_file(content: &str) -> Self {
        Self::new(parse_tle(content))
    }
    
    pub fn satellite_count(&self) -> usize {
        self.tles.len()
    }
    
    pub fn get_satellites(&self) -> Vec<String> {
        self.tles.keys().cloned().collect()
    }
    
    /// Compute satellite position at given epoch
    pub fn compute_position(&self, sat_id: &str, epoch: &Epoch) -> Option<Ecef> {
        let tle = self.tles.get(sat_id)?;
        
        // Time since TLE epoch in minutes
        let tle_epoch = tle.get_epoch();
        let dt_sec = epoch.diff_seconds(&tle_epoch);
        let dt_min = dt_sec / 60.0;
        
        // Limit validity to ±7 days
        if dt_min.abs() > 10080.0 {
            return None;
        }
        
        // Convert to radians
        let incl = tle.inclination.to_radians();
        let raan0 = tle.raan.to_radians();
        let argp0 = tle.arg_perigee.to_radians();
        let ma0 = tle.mean_anomaly.to_radians();
        let ecc = tle.eccentricity;
        
        // Mean motion in rad/min
        let n0 = tle.mean_motion * 2.0 * PI / 1440.0;
        
        // Semi-major axis from mean motion
        let a = (GM / (n0 / 60.0).powi(2)).powf(1.0 / 3.0);
        
        // J2 secular perturbations
        let cos_i = incl.cos();
        let sin_i = incl.sin();
        let p = a * (1.0 - ecc * ecc);
        let j2_factor = 1.5 * J2 * (EARTH_RADIUS / p).powi(2);
        
        // Secular rates
        let raan_dot = -j2_factor * n0 * cos_i;
        let argp_dot = j2_factor * n0 * (2.0 - 2.5 * sin_i * sin_i);
        let ma_dot = n0 * (1.0 + j2_factor * (1.0 - 1.5 * sin_i * sin_i) * (1.0 - ecc * ecc).sqrt());
        
        // Propagate
        let raan = raan0 + raan_dot * dt_min;
        let argp = argp0 + argp_dot * dt_min;
        let ma = ma0 + ma_dot * dt_min;
        
        // Solve Kepler's equation for eccentric anomaly
        let mut ea = ma;
        for _ in 0..10 {
            ea = ma + ecc * ea.sin();
        }
        
        // True anomaly
        let sin_ea = ea.sin();
        let cos_ea = ea.cos();
        let nu = ((1.0 - ecc * ecc).sqrt() * sin_ea).atan2(cos_ea - ecc);
        
        // Distance
        let r = a * (1.0 - ecc * cos_ea);
        
        // Position in orbital plane
        let u = argp + nu;
        let x_orb = r * u.cos();
        let y_orb = r * u.sin();
        
        // Rotate to ECEF
        let cos_raan = raan.cos();
        let sin_raan = raan.sin();
        let cos_i = incl.cos();
        let sin_i = incl.sin();
        
        // ECI position
        let x_eci = x_orb * cos_raan - y_orb * cos_i * sin_raan;
        let y_eci = x_orb * sin_raan + y_orb * cos_i * cos_raan;
        let z_eci = y_orb * sin_i;
        
        // ECI to ECEF (account for Earth rotation)
        let gmst = compute_gmst(epoch);
        let cos_gmst = gmst.cos();
        let sin_gmst = gmst.sin();
        
        let x = x_eci * cos_gmst + y_eci * sin_gmst;
        let y = -x_eci * sin_gmst + y_eci * cos_gmst;
        let z = z_eci;
        
        Some(Ecef::new(x, y, z))
    }
}

/// Compute Greenwich Mean Sidereal Time
fn compute_gmst(epoch: &Epoch) -> f64 {
    let jd = epoch.julian_date();
    let t = (jd - 2451545.0) / 36525.0;
    
    // GMST in degrees
    let gmst_deg = 280.46061837 + 360.98564736629 * (jd - 2451545.0)
        + 0.000387933 * t * t
        - t * t * t / 38710000.0;
    
    (gmst_deg % 360.0).to_radians()
}

/// Download TLE data from CelesTrak
pub fn download_gnss_tle() -> Result<String, String> {
    // CelesTrak GNSS TLE URL (no auth required)
    let urls = [
        "https://celestrak.org/NORAD/elements/gp.php?GROUP=gnss&FORMAT=tle",
        "https://celestrak.org/NORAD/elements/gnss.txt",
        "https://www.celestrak.com/NORAD/elements/gnss.txt",
    ];
    
    for url in &urls {
        match download_url(url) {
            Ok(content) => {
                // Verify it looks like TLE data
                if content.contains("1 ") && content.contains("2 ") {
                    return Ok(content);
                }
            }
            Err(_) => continue,
        }
    }
    
    Err("Failed to download TLE from all sources".to_string())
}

/// Simple HTTP GET (no external dependencies)
fn download_url(url: &str) -> Result<String, String> {
    // Use std::process to call curl as fallback
    // In a real implementation, you'd use reqwest or ureq
    
    #[cfg(unix)]
    {
        use std::process::Command;
        
        let output = Command::new("curl")
            .args(&["-sL", "--connect-timeout", "10", url])
            .output()
            .map_err(|e| e.to_string())?;
        
        if output.status.success() {
            String::from_utf8(output.stdout).map_err(|e| e.to_string())
        } else {
            Err("curl failed".to_string())
        }
    }
    
    #[cfg(windows)]
    {
        use std::process::Command;
        
        // Try PowerShell on Windows
        let output = Command::new("powershell")
            .args(&["-Command", &format!("(Invoke-WebRequest -Uri '{}' -TimeoutSec 10).Content", url)])
            .output()
            .map_err(|e| e.to_string())?;
        
        if output.status.success() {
            String::from_utf8(output.stdout).map_err(|e| e.to_string())
        } else {
            Err("PowerShell download failed".to_string())
        }
    }
    
    #[cfg(not(any(unix, windows)))]
    {
        Err("HTTP download not supported on this platform".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_tle() {
        let tle_str = r#"GPS BIIR-2  (PRN 13)
1 24876U 97035A   24001.50000000  .00000000  00000-0  00000-0 0  9999
2 24876  55.4408 248.0576 0044516 114.5248 245.9563  2.00563104194565"#;
        
        let tles = parse_tle(tle_str);
        assert_eq!(tles.len(), 1);
        assert_eq!(tles[0].satellite_id, "G13");
    }
}
