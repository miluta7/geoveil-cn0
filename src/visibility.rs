//! Satellite Visibility Prediction
//!
//! Predicts which satellites should be visible at a given location/time.
//! 
//! **Primary method**: BRDC navigation ephemeris (most accurate)
//! **Fallback method**: TLE + simplified SGP4 (when no BRDC available)
//!
//! Compares predicted vs observed satellites to detect:
//! - Jamming (missing satellites)
//! - Spoofing (unexpected satellites)
//!
//! Author: Miluta Dulea-Flueras
//! Date: January 2026

use crate::types::*;
use crate::navigation::{NavigationData, compute_satellite_position, calculate_azel};
use crate::tle::{SimplifiedSgp4, download_gnss_tle};
use std::collections::HashMap;
use serde::{Deserialize, Serialize};

/// Satellite visibility status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VisibilityStatus {
    /// Predicted and observed - normal
    Confirmed,
    /// Predicted but not observed - possible jamming/obstruction
    Missing,
    /// Observed but not predicted - possible spoofing/multipath
    Unexpected,
    /// Below elevation mask
    BelowMask,
}

impl VisibilityStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Confirmed => "confirmed",
            Self::Missing => "missing",
            Self::Unexpected => "unexpected",
            Self::BelowMask => "below_mask",
        }
    }
}

/// Predicted satellite visibility at a single epoch
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SatelliteVisibility {
    pub satellite: String,
    pub system: String,
    pub azimuth: f64,
    pub elevation: f64,
    pub status: String,
    pub cn0: Option<f64>,
}

/// Visibility prediction result for a single epoch
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpochVisibility {
    pub timestamp: String,
    pub predicted_count: usize,
    pub observed_count: usize,
    pub confirmed_count: usize,
    pub missing_count: usize,
    pub unexpected_count: usize,
    pub satellites: Vec<SatelliteVisibility>,
}

/// Overall visibility assessment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisibilityAssessment {
    /// Per-epoch visibility data
    pub epochs: Vec<EpochVisibility>,
    
    /// Summary statistics
    pub summary: VisibilitySummary,
    
    /// Anomaly events (periods with missing/unexpected satellites)
    pub anomalies: Vec<VisibilityAnomaly>,
}

/// Summary of visibility assessment
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VisibilitySummary {
    pub total_epochs: usize,
    pub mean_predicted: f64,
    pub mean_observed: f64,
    pub mean_confirmed: f64,
    pub mean_missing: f64,
    pub mean_unexpected: f64,
    
    /// Percentage of predicted satellites that were observed
    pub confirmation_rate: f64,
    
    /// Per-system breakdown (key = system name like "GPS")
    pub by_system: HashMap<String, SystemVisibilitySummary>,
    
    /// Satellites that were frequently missing (possible obstructions)
    pub frequently_missing: Vec<String>,
    
    /// Satellites that appeared unexpectedly (possible spoofing)
    pub frequently_unexpected: Vec<String>,
    
    /// Data source used for prediction
    pub prediction_source: String,
}

/// Per-system visibility summary
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SystemVisibilitySummary {
    pub system: String,
    pub predicted_satellites: usize,
    pub observed_satellites: usize,
    pub confirmation_rate: f64,
    pub missing_satellites: Vec<String>,
    pub unexpected_satellites: Vec<String>,
}

/// Visibility anomaly event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisibilityAnomaly {
    pub start_time: String,
    pub end_time: String,
    pub duration_seconds: f64,
    pub anomaly_type: String,  // "missing_satellites" or "unexpected_satellites"
    pub severity: String,      // "low", "high", "critical"
    pub affected_satellites: Vec<String>,
    pub affected_systems: Vec<String>,
    pub description: String,
}

/// Satellite position source
#[derive(Debug, Clone)]
pub enum SatelliteSource {
    /// BRDC navigation ephemeris (primary, most accurate)
    Navigation(NavigationData),
    /// TLE + simplified SGP4 (fallback)
    Tle(SimplifiedSgp4),
    /// No satellite data available
    None,
}

/// Visibility Predictor
pub struct VisibilityPredictor {
    source: SatelliteSource,
    elevation_mask: f64,
    /// All satellites available for prediction
    available_satellites: Vec<String>,
    /// Source description
    source_description: String,
}

impl VisibilityPredictor {
    /// Create new predictor from navigation data (primary method)
    pub fn new(nav_data: NavigationData, elevation_mask: f64) -> Self {
        // Collect all satellites from navigation data
        let mut available_satellites = Vec::new();
        
        for sat_id in nav_data.gps_ephemeris.keys() {
            available_satellites.push(sat_id.clone());
        }
        for sat_id in nav_data.galileo_ephemeris.keys() {
            available_satellites.push(sat_id.clone());
        }
        for sat_id in nav_data.beidou_ephemeris.keys() {
            available_satellites.push(sat_id.clone());
        }
        for sat_id in nav_data.glonass_ephemeris.keys() {
            available_satellites.push(sat_id.clone());
        }
        
        available_satellites.sort();
        
        let source_description = format!(
            "BRDC Navigation ({} satellites)",
            available_satellites.len()
        );
        
        Self {
            source: SatelliteSource::Navigation(nav_data),
            elevation_mask,
            available_satellites,
            source_description,
        }
    }
    
    /// Create predictor from TLE data (fallback method)
    pub fn from_tle(tle_content: &str, elevation_mask: f64) -> Self {
        let sgp4 = SimplifiedSgp4::from_tle_file(tle_content);
        let available_satellites = sgp4.get_satellites();
        
        let source_description = format!(
            "TLE/SGP4 ({} satellites)",
            available_satellites.len()
        );
        
        Self {
            source: SatelliteSource::Tle(sgp4),
            elevation_mask,
            available_satellites,
            source_description,
        }
    }
    
    /// Create predictor with automatic TLE download (fallback with auto-download)
    pub fn from_tle_auto(elevation_mask: f64) -> Result<Self, String> {
        let tle_content = download_gnss_tle()?;
        Ok(Self::from_tle(&tle_content, elevation_mask))
    }
    
    /// Create predictor trying BRDC first, then TLE fallback
    pub fn new_with_fallback(
        nav_data: Option<NavigationData>,
        tle_path: Option<&str>,
        auto_download_tle: bool,
        elevation_mask: f64,
    ) -> Self {
        // Try BRDC navigation first (most accurate)
        if let Some(nav) = nav_data {
            if nav.has_data() {
                return Self::new(nav, elevation_mask);
            }
        }
        
        // Try TLE file if provided
        if let Some(path) = tle_path {
            if let Ok(content) = std::fs::read_to_string(path) {
                let predictor = Self::from_tle(&content, elevation_mask);
                if !predictor.available_satellites.is_empty() {
                    return predictor;
                }
            }
        }
        
        // Try auto-download TLE
        if auto_download_tle {
            if let Ok(predictor) = Self::from_tle_auto(elevation_mask) {
                if !predictor.available_satellites.is_empty() {
                    return predictor;
                }
            }
        }
        
        // No satellite data available
        Self {
            source: SatelliteSource::None,
            elevation_mask,
            available_satellites: Vec::new(),
            source_description: "No satellite data available".to_string(),
        }
    }
    
    /// Get source description
    pub fn source_description(&self) -> &str {
        &self.source_description
    }
    
    /// Check if predictor has satellite data
    pub fn has_data(&self) -> bool {
        !self.available_satellites.is_empty()
    }
    
    /// Compute satellite position based on source
    fn compute_position(&self, sat_id: &str, epoch: &Epoch) -> Option<Ecef> {
        match &self.source {
            SatelliteSource::Navigation(nav) => {
                compute_satellite_position(nav, sat_id, epoch)
            }
            SatelliteSource::Tle(sgp4) => {
                sgp4.compute_position(sat_id, epoch)
            }
            SatelliteSource::None => None,
        }
    }
    
    /// Predict visible satellites at a given epoch and receiver position
    pub fn predict_visible_at_epoch(
        &self,
        receiver_pos: &Ecef,
        epoch: &Epoch,
    ) -> Vec<(String, f64, f64)> {
        // Returns: (satellite_id, azimuth, elevation)
        let mut visible = Vec::new();
        
        for sat_id in &self.available_satellites {
            if let Some(sat_pos) = self.compute_position(sat_id, epoch) {
                // Validate satellite position (should be ~20,200km from Earth center for MEO)
                let sat_dist = sat_pos.magnitude();
                if sat_dist < 20_000_000.0 || sat_dist > 50_000_000.0 {
                    // Invalid position - skip this satellite
                    continue;
                }
                
                let (az, el) = calculate_azel(receiver_pos, &sat_pos);
                
                // Validate elevation is reasonable (-90 to +90)
                if el.is_nan() || el < -90.0 || el > 90.0 {
                    continue;
                }
                
                // Apply elevation mask
                if el >= self.elevation_mask {
                    visible.push((sat_id.clone(), az, el));
                }
            }
        }
        
        visible
    }
    
    /// Full visibility assessment comparing predicted vs observed
    pub fn assess_visibility(
        &self,
        obs_data: &RinexObsData,
    ) -> VisibilityAssessment {
        let receiver_pos = match &obs_data.approx_position {
            Some(pos) => pos.clone(),
            None => {
                // No receiver position - cannot predict
                return VisibilityAssessment {
                    epochs: Vec::new(),
                    summary: VisibilitySummary::default(),
                    anomalies: Vec::new(),
                };
            }
        };
        
        let mut epoch_results = Vec::new();
        
        // Track per-satellite statistics
        let mut sat_predicted_count: HashMap<String, usize> = HashMap::new();
        let mut sat_observed_count: HashMap<String, usize> = HashMap::new();
        let mut sat_missing_count: HashMap<String, usize> = HashMap::new();
        let mut sat_unexpected_count: HashMap<String, usize> = HashMap::new();
        
        // Process each epoch
        for epoch_data in &obs_data.epochs {
            let timestamp = epoch_data.epoch.to_iso_string();
            
            // Get predicted satellites
            let predicted = self.predict_visible_at_epoch(&receiver_pos, &epoch_data.epoch);
            let predicted_set: HashMap<String, (f64, f64)> = predicted.iter()
                .map(|(id, az, el)| (id.clone(), (*az, *el)))
                .collect();
            
            // Get observed satellites (those with CN0/SNR data)
            let mut observed_set: HashMap<String, f64> = HashMap::new();
            for (sat_id, sat_obs) in &epoch_data.observations {
                // Check if we have CN0 data for this satellite
                for (obs_code, obs_val) in &sat_obs.values {
                    if obs_code.starts_with('S') && obs_val.value > 0.0 {
                        observed_set.insert(sat_id.clone(), obs_val.value);
                        break;
                    }
                }
            }
            
            // Build visibility list
            let mut satellites = Vec::new();
            let mut confirmed = 0usize;
            let mut missing = 0usize;
            let mut unexpected = 0usize;
            
            // Check predicted satellites
            for (sat_id, (az, el)) in &predicted_set {
                *sat_predicted_count.entry(sat_id.clone()).or_insert(0) += 1;
                
                let sys = sat_id_to_system_name(sat_id);
                
                if let Some(cn0) = observed_set.get(sat_id) {
                    // Confirmed - predicted and observed
                    satellites.push(SatelliteVisibility {
                        satellite: sat_id.clone(),
                        system: sys,
                        azimuth: *az,
                        elevation: *el,
                        status: VisibilityStatus::Confirmed.as_str().to_string(),
                        cn0: Some(*cn0),
                    });
                    confirmed += 1;
                    *sat_observed_count.entry(sat_id.clone()).or_insert(0) += 1;
                } else {
                    // Missing - predicted but not observed
                    satellites.push(SatelliteVisibility {
                        satellite: sat_id.clone(),
                        system: sys,
                        azimuth: *az,
                        elevation: *el,
                        status: VisibilityStatus::Missing.as_str().to_string(),
                        cn0: None,
                    });
                    missing += 1;
                    *sat_missing_count.entry(sat_id.clone()).or_insert(0) += 1;
                }
            }
            
            // Check for unexpected satellites (observed but not predicted)
            for (sat_id, cn0) in &observed_set {
                if !predicted_set.contains_key(sat_id) {
                    let sys = sat_id_to_system_name(sat_id);
                    
                    // Try to compute position anyway for az/el
                    let (az, el) = if let Some(sat_pos) = self.compute_position(sat_id, &epoch_data.epoch) {
                        calculate_azel(&receiver_pos, &sat_pos)
                    } else {
                        (0.0, 0.0)
                    };
                    
                    satellites.push(SatelliteVisibility {
                        satellite: sat_id.clone(),
                        system: sys,
                        azimuth: az,
                        elevation: el,
                        status: VisibilityStatus::Unexpected.as_str().to_string(),
                        cn0: Some(*cn0),
                    });
                    unexpected += 1;
                    *sat_unexpected_count.entry(sat_id.clone()).or_insert(0) += 1;
                }
            }
            
            epoch_results.push(EpochVisibility {
                timestamp,
                predicted_count: predicted_set.len(),
                observed_count: observed_set.len(),
                confirmed_count: confirmed,
                missing_count: missing,
                unexpected_count: unexpected,
                satellites,
            });
        }
        
        // Build summary
        let summary = self.build_summary(
            &epoch_results,
            &sat_predicted_count,
            &sat_observed_count,
            &sat_missing_count,
            &sat_unexpected_count,
        );
        
        // Detect anomalies
        let anomalies = self.detect_visibility_anomalies(&epoch_results);
        
        VisibilityAssessment {
            epochs: epoch_results,
            summary,
            anomalies,
        }
    }
    
    fn build_summary(
        &self,
        epochs: &[EpochVisibility],
        _sat_predicted: &HashMap<String, usize>,
        _sat_observed: &HashMap<String, usize>,
        sat_missing: &HashMap<String, usize>,
        sat_unexpected: &HashMap<String, usize>,
    ) -> VisibilitySummary {
        if epochs.is_empty() {
            return VisibilitySummary::default();
        }
        
        let n = epochs.len() as f64;
        
        let mean_predicted = epochs.iter().map(|e| e.predicted_count as f64).sum::<f64>() / n;
        let mean_observed = epochs.iter().map(|e| e.observed_count as f64).sum::<f64>() / n;
        let mean_confirmed = epochs.iter().map(|e| e.confirmed_count as f64).sum::<f64>() / n;
        let mean_missing = epochs.iter().map(|e| e.missing_count as f64).sum::<f64>() / n;
        let mean_unexpected = epochs.iter().map(|e| e.unexpected_count as f64).sum::<f64>() / n;
        
        let confirmation_rate = if mean_predicted > 0.0 {
            mean_confirmed / mean_predicted * 100.0
        } else {
            0.0
        };
        
        // Per-system breakdown - compute MEAN predicted/observed per epoch
        // This gives realistic "how many satellites visible at any moment" counts
        let mut by_system: HashMap<String, SystemVisibilitySummary> = HashMap::new();
        
        for sys_char in &['G', 'R', 'E', 'C'] {
            let sys_name = match sys_char {
                'G' => "GPS",
                'R' => "GLONASS",
                'E' => "Galileo",
                'C' => "BeiDou",
                _ => continue,
            }.to_string();
            
            let sys_prefix = sys_char.to_string();
            
            // Count predicted and observed per epoch, then compute MEAN
            // This gives "typical" satellite count at any moment, not cumulative unique
            let mut predicted_per_epoch: Vec<usize> = Vec::new();
            let mut observed_per_epoch: Vec<usize> = Vec::new();
            let mut unique_observed: std::collections::HashSet<String> = std::collections::HashSet::new();
            
            for epoch in epochs {
                let mut epoch_predicted = 0usize;
                let mut epoch_observed = 0usize;
                
                for sat in &epoch.satellites {
                    if sat.satellite.starts_with(&sys_prefix) {
                        if sat.status == "confirmed" || sat.status == "missing" {
                            epoch_predicted += 1;
                        }
                        if sat.status == "confirmed" || sat.status == "unexpected" {
                            epoch_observed += 1;
                            unique_observed.insert(sat.satellite.clone());
                        }
                    }
                }
                
                predicted_per_epoch.push(epoch_predicted);
                observed_per_epoch.push(epoch_observed);
            }
            
            let n = epochs.len().max(1);
            
            // Use MEAN per epoch (rounded up) - this is realistic "expected at any moment"
            let mean_predicted = if !predicted_per_epoch.is_empty() {
                (predicted_per_epoch.iter().sum::<usize>() as f64 / predicted_per_epoch.len() as f64).ceil() as usize
            } else {
                0
            };
            
            // For observed, use unique count (how many different satellites were tracked)
            let unique_observed_count = unique_observed.len();
            
            // Frequently missing/unexpected (>25% of epochs)
            let threshold = n / 4;
            let missing: Vec<String> = sat_missing.iter()
                .filter(|(s, count)| s.starts_with(&sys_prefix) && **count > threshold)
                .map(|(s, _)| s.clone())
                .collect();
            let unexpected: Vec<String> = sat_unexpected.iter()
                .filter(|(s, count)| s.starts_with(&sys_prefix) && **count > threshold)
                .map(|(s, _)| s.clone())
                .collect();
            
            // Confirmation rate: observed / predicted (capped at 100%)
            let conf_rate = if mean_predicted > 0 {
                (unique_observed_count as f64 / mean_predicted as f64 * 100.0).min(100.0)
            } else {
                0.0
            };
            
            by_system.insert(sys_name.clone(), SystemVisibilitySummary {
                system: sys_name,
                predicted_satellites: mean_predicted,          // MEAN predicted per epoch (typical visibility)
                observed_satellites: unique_observed_count,    // UNIQUE satellites observed (tracked)
                confirmation_rate: conf_rate,
                missing_satellites: missing,
                unexpected_satellites: unexpected,
            });
        }
        
        // Frequently missing/unexpected satellites (>25% of epochs)
        let threshold = epochs.len() / 4;
        
        let frequently_missing: Vec<String> = sat_missing.iter()
            .filter(|(_, count)| **count > threshold)
            .map(|(s, _)| s.clone())
            .collect();
        
        let frequently_unexpected: Vec<String> = sat_unexpected.iter()
            .filter(|(_, count)| **count > threshold)
            .map(|(s, _)| s.clone())
            .collect();
        
        VisibilitySummary {
            total_epochs: epochs.len(),
            mean_predicted,
            mean_observed,
            mean_confirmed,
            mean_missing,
            mean_unexpected,
            confirmation_rate,
            by_system,
            frequently_missing,
            frequently_unexpected,
            prediction_source: self.source_description.clone(),
        }
    }
    
    fn detect_visibility_anomalies(&self, epochs: &[EpochVisibility]) -> Vec<VisibilityAnomaly> {
        let mut anomalies = Vec::new();
        
        if epochs.len() < 2 {
            return anomalies;
        }
        
        // Detect periods with high missing satellite counts
        let mut in_missing_anomaly = false;
        let mut missing_start_idx = 0;
        let mut missing_sats: Vec<String> = Vec::new();
        
        for (i, epoch) in epochs.iter().enumerate() {
            let missing_ratio = if epoch.predicted_count > 0 {
                epoch.missing_count as f64 / epoch.predicted_count as f64
            } else {
                0.0
            };
            
            // Start anomaly if >30% missing
            if missing_ratio > 0.3 && !in_missing_anomaly {
                in_missing_anomaly = true;
                missing_start_idx = i;
                missing_sats.clear();
            }
            
            if in_missing_anomaly {
                for sat in &epoch.satellites {
                    if sat.status == "missing" && !missing_sats.contains(&sat.satellite) {
                        missing_sats.push(sat.satellite.clone());
                    }
                }
            }
            
            // End anomaly
            if missing_ratio <= 0.1 && in_missing_anomaly {
                in_missing_anomaly = false;
                
                if i > missing_start_idx {
                    let start = &epochs[missing_start_idx];
                    let end = &epochs[i - 1];
                    
                    let duration = if let (Some(s), Some(e)) = (
                        Epoch::parse(&start.timestamp),
                        Epoch::parse(&end.timestamp)
                    ) {
                        e.diff_seconds(&s).abs()
                    } else {
                        (i - missing_start_idx) as f64 * 30.0 // Assume 30s interval
                    };
                    
                    // Deduplicate affected systems
                    let mut affected_systems: Vec<String> = missing_sats.iter()
                        .map(|s| sat_id_to_system_name(s))
                        .collect();
                    affected_systems.sort();
                    affected_systems.dedup();
                    
                    let severity = if missing_sats.len() > 10 || affected_systems.len() > 2 {
                        "critical"
                    } else if missing_sats.len() > 5 {
                        "high"
                    } else {
                        "low"
                    };
                    
                    anomalies.push(VisibilityAnomaly {
                        start_time: start.timestamp.clone(),
                        end_time: end.timestamp.clone(),
                        duration_seconds: duration,
                        anomaly_type: "missing_satellites".to_string(),
                        severity: severity.to_string(),
                        affected_satellites: missing_sats.clone(),
                        affected_systems: affected_systems.clone(),
                        description: format!(
                            "{} satellites missing for {:.0}s - possible jamming or obstruction",
                            missing_sats.len(),
                            duration
                        ),
                    });
                }
            }
        }
        
        // Detect periods with unexpected satellites (possible spoofing)
        let mut in_unexpected_anomaly = false;
        let mut unexpected_start_idx = 0;
        let mut unexpected_sats: Vec<String> = Vec::new();
        
        for (i, epoch) in epochs.iter().enumerate() {
            if epoch.unexpected_count > 2 && !in_unexpected_anomaly {
                in_unexpected_anomaly = true;
                unexpected_start_idx = i;
                unexpected_sats.clear();
            }
            
            if in_unexpected_anomaly {
                for sat in &epoch.satellites {
                    if sat.status == "unexpected" && !unexpected_sats.contains(&sat.satellite) {
                        unexpected_sats.push(sat.satellite.clone());
                    }
                }
            }
            
            if epoch.unexpected_count <= 1 && in_unexpected_anomaly {
                in_unexpected_anomaly = false;
                
                if i > unexpected_start_idx {
                    let start = &epochs[unexpected_start_idx];
                    let end = &epochs[i - 1];
                    
                    let duration = if let (Some(s), Some(e)) = (
                        Epoch::parse(&start.timestamp),
                        Epoch::parse(&end.timestamp)
                    ) {
                        e.diff_seconds(&s).abs()
                    } else {
                        (i - unexpected_start_idx) as f64 * 30.0
                    };
                    
                    // Deduplicate affected systems
                    let mut affected_systems: Vec<String> = unexpected_sats.iter()
                        .map(|s| sat_id_to_system_name(s))
                        .collect();
                    affected_systems.sort();
                    affected_systems.dedup();
                    
                    let severity = if unexpected_sats.len() > 5 {
                        "critical"
                    } else if unexpected_sats.len() > 2 {
                        "high"
                    } else {
                        "low"
                    };
                    
                    anomalies.push(VisibilityAnomaly {
                        start_time: start.timestamp.clone(),
                        end_time: end.timestamp.clone(),
                        duration_seconds: duration,
                        anomaly_type: "unexpected_satellites".to_string(),
                        severity: severity.to_string(),
                        affected_satellites: unexpected_sats.clone(),
                        affected_systems: affected_systems.clone(),
                        description: format!(
                            "{} unexpected satellites for {:.0}s - possible spoofing or multipath",
                            unexpected_sats.len(),
                            duration
                        ),
                    });
                }
            }
        }
        
        anomalies
    }
    
    /// Get list of all satellites in navigation data
    pub fn get_available_satellites(&self) -> &[String] {
        &self.available_satellites
    }
}

fn sat_id_to_system_name(sat_id: &str) -> String {
    match sat_id.chars().next() {
        Some('G') => "GPS".to_string(),
        Some('R') => "GLONASS".to_string(),
        Some('E') => "Galileo".to_string(),
        Some('C') => "BeiDou".to_string(),
        Some('J') => "QZSS".to_string(),
        Some('S') => "SBAS".to_string(),
        _ => "Unknown".to_string(),
    }
}

/// Convert VisibilityAssessment to JSON
impl VisibilityAssessment {
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|e| {
            format!("{{\"error\": \"{}\"}}", e)
        })
    }
    
    pub fn to_json_pretty(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|e| {
            format!("{{\"error\": \"{}\"}}", e)
        })
    }
}
