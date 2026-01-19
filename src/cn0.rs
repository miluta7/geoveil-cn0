//! CN0 Analysis Engine
//!
//! Extracts CN0/SNR values, computes statistics, detects anomalies

use crate::types::*;
use crate::navigation::{NavigationData, compute_satellite_position, calculate_azel};
use crate::visibility::VisibilityPredictor;
use std::collections::HashMap;

/// CN0 Analyzer
pub struct CN0Analyzer {
    obs_data: RinexObsData,
    nav_data: Option<NavigationData>,
    config: AnalysisConfig,
}

impl CN0Analyzer {
    pub fn new(obs_data: RinexObsData, config: AnalysisConfig) -> Self {
        Self {
            obs_data,
            nav_data: None,
            config,
        }
    }
    
    pub fn with_navigation(mut self, nav_data: NavigationData) -> Self {
        self.nav_data = Some(nav_data);
        self
    }
    
    /// Run full CN0 analysis
    pub fn analyze(&self) -> AnalysisResult {
        let mut result = AnalysisResult::default();
        
        // File info
        result.file_info = self.build_file_info();
        
        // Extract all CN0 observations
        let observations = self.extract_cn0_observations();
        
        if observations.is_empty() {
            return result;
        }
        
        // Run visibility assessment FIRST if navigation data available
        // This gives us predicted satellite counts per system
        let visibility_data = if let Some(nav_data) = &self.nav_data {
            Some(self.run_visibility_assessment(nav_data))
        } else {
            None
        };
        
        // Build summary
        result.summary = self.build_summary(&observations);
        
        // Build satellite statistics
        result.satellite_stats = self.build_satellite_stats(&observations);
        
        // Build constellation statistics - use visibility predictions for expected counts
        result.constellation_stats = self.build_constellation_stats(&observations, visibility_data.as_ref());
        
        // Build timeseries data
        result.timeseries = self.build_timeseries(&observations);
        
        // Build skyplot data
        result.skyplot = self.build_skyplot(&observations);
        
        // Build heatmap data
        result.heatmap = self.build_heatmap(&observations);
        
        // Detect anomalies
        result.anomalies = self.detect_anomalies(&observations);
        
        // Update anomaly counts
        result.summary.anomaly_count = result.anomalies.len();
        result.summary.critical_count = result.anomalies.iter().filter(|a| a.severity == "critical").count();
        result.summary.high_count = result.anomalies.iter().filter(|a| a.severity == "high").count();
        result.summary.low_count = result.anomalies.iter().filter(|a| a.severity == "low").count();
        
        // Store visibility data
        result.visibility = visibility_data;
        
        // Compute quality score
        result.quality_score = self.compute_quality_score(&observations, &result);
        
        result
    }
    
    /// Run visibility assessment comparing predicted vs observed satellites
    fn run_visibility_assessment(&self, nav_data: &NavigationData) -> VisibilityData {
        let predictor = VisibilityPredictor::new(nav_data.clone(), self.config.min_elevation);
        let assessment = predictor.assess_visibility(&self.obs_data);
        
        // Convert to VisibilityData
        let mut by_system: HashMap<String, SystemVisibilityStats> = HashMap::new();
        for (sys_name, sys_stats) in &assessment.summary.by_system {
            by_system.insert(sys_name.clone(), SystemVisibilityStats {
                system: sys_stats.system.clone(),
                predicted_satellites: sys_stats.predicted_satellites,
                observed_satellites: sys_stats.observed_satellites,
                confirmation_rate: sys_stats.confirmation_rate,
                missing_satellites: sys_stats.missing_satellites.clone(),
                unexpected_satellites: sys_stats.unexpected_satellites.clone(),
            });
        }
        
        let anomalies: Vec<VisibilityAnomalyInfo> = assessment.anomalies.iter().map(|a| {
            VisibilityAnomalyInfo {
                start_time: a.start_time.clone(),
                end_time: a.end_time.clone(),
                duration_seconds: a.duration_seconds,
                anomaly_type: a.anomaly_type.clone(),
                severity: a.severity.clone(),
                affected_satellites: a.affected_satellites.clone(),
                affected_systems: a.affected_systems.clone(),
                description: a.description.clone(),
            }
        }).collect();
        
        VisibilityData {
            has_prediction: true,
            prediction_source: assessment.summary.prediction_source.clone(),
            confirmation_rate: assessment.summary.confirmation_rate,
            mean_predicted: assessment.summary.mean_predicted,
            mean_observed: assessment.summary.mean_observed,
            mean_missing: assessment.summary.mean_missing,
            mean_unexpected: assessment.summary.mean_unexpected,
            frequently_missing: assessment.summary.frequently_missing.clone(),
            frequently_unexpected: assessment.summary.frequently_unexpected.clone(),
            by_system,
            anomalies,
        }
    }
    
    fn build_file_info(&self) -> FileInfo {
        let mut info = FileInfo::default();
        
        info.marker_name = self.obs_data.marker_name.clone();
        info.receiver_type = self.obs_data.receiver_type.clone();
        info.antenna_type = self.obs_data.antenna_type.clone();
        info.interval = self.obs_data.interval;
        
        if let Some(first) = self.obs_data.first_epoch() {
            info.start_time = first.to_iso_string();
        }
        if let Some(last) = self.obs_data.last_epoch() {
            info.end_time = last.to_iso_string();
        }
        
        if let (Some(first), Some(last)) = (self.obs_data.first_epoch(), self.obs_data.last_epoch()) {
            info.duration_hours = last.diff_seconds(first) / 3600.0;
        }
        
        if let Some(pos) = &self.obs_data.approx_position {
            let (lat, lon, height) = pos.to_geodetic();
            info.position_lat = lat;
            info.position_lon = lon;
            info.position_height = height;
        }
        
        info
    }
    
    fn extract_cn0_observations(&self) -> Vec<CN0Observation> {
        let mut observations = Vec::new();
        
        let receiver_pos = self.obs_data.approx_position.clone();
        
        for epoch_data in &self.obs_data.epochs {
            let timestamp = epoch_data.epoch.to_iso_string();
            
            for (sat_id, sat_obs) in &epoch_data.observations {
                let sys_char = sat_id.chars().next().unwrap_or('G');
                
                // Check if system is enabled
                if !self.config.systems.contains(&sys_char) {
                    continue;
                }
                
                let prn: u32 = sat_id[1..].trim().parse().unwrap_or(0);
                let system = match sys_char {
                    'G' => "GPS",
                    'R' => "GLONASS",
                    'E' => "Galileo",
                    'C' => "BeiDou",
                    'J' => "QZSS",
                    'S' => "SBAS",
                    _ => "Unknown",
                }.to_string();
                
                // Compute azimuth and elevation if we have navigation data
                let (azimuth, elevation) = if let (Some(nav), Some(rx_pos)) = (&self.nav_data, &receiver_pos) {
                    if let Some(sat_pos) = compute_satellite_position(nav, sat_id, &epoch_data.epoch) {
                        calculate_azel(rx_pos, &sat_pos)
                    } else {
                        (0.0, 45.0) // Default if no ephemeris
                    }
                } else {
                    (0.0, 45.0) // Default if no navigation
                };
                
                // Skip low elevation satellites
                if elevation < self.config.min_elevation {
                    continue;
                }
                
                // Extract CN0/SNR values for all signals
                for (obs_code, obs_val) in &sat_obs.values {
                    // CN0/SNR observation codes start with 'S'
                    if obs_code.starts_with('S') {
                        let cn0 = obs_val.value;
                        
                        // Basic validation
                        if cn0 < self.config.min_cn0 || cn0 > self.config.max_cn0 {
                            continue;
                        }
                        
                        observations.push(CN0Observation {
                            timestamp: timestamp.clone(),
                            satellite: sat_id.clone(),
                            system: system.clone(),
                            prn,
                            signal: obs_code.clone(),
                            cn0,
                            elevation,
                            azimuth,
                        });
                    }
                }
            }
        }
        
        observations
    }
    
    fn build_summary(&self, observations: &[CN0Observation]) -> AnalysisSummary {
        let mut summary = AnalysisSummary::default();
        
        summary.total_observations = observations.len();
        summary.total_epochs = self.obs_data.num_epochs();
        
        // Count unique satellites from RINEX (all satellites, not just those with SNR)
        summary.total_satellites = self.obs_data.num_satellites();
        
        // Systems observed (from RINEX)
        let all_sats_by_system = self.obs_data.satellites_by_system();
        let mut systems: Vec<String> = Vec::new();
        for sys_char in all_sats_by_system.keys() {
            let system = match sys_char.as_str() {
                "G" => "GPS",
                "R" => "GLONASS",
                "E" => "Galileo",
                "C" => "BeiDou",
                "J" => "QZSS",
                "S" => "SBAS",
                "I" => "IRNSS",
                _ => sys_char.as_str(),
            }.to_string();
            if !systems.contains(&system) {
                systems.push(system);
            }
        }
        summary.systems_observed = systems;
        
        // CN0 statistics
        if !observations.is_empty() {
            let cn0_values: Vec<f64> = observations.iter().map(|o| o.cn0).collect();
            summary.mean_cn0 = cn0_values.iter().sum::<f64>() / cn0_values.len() as f64;
            summary.min_cn0 = cn0_values.iter().cloned().fold(f64::INFINITY, f64::min);
            summary.max_cn0 = cn0_values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        }
        
        summary
    }
    
    fn build_satellite_stats(&self, observations: &[CN0Observation]) -> HashMap<String, SatelliteStats> {
        let mut stats: HashMap<String, SatelliteStats> = HashMap::new();
        
        // Group by satellite
        let mut sat_obs: HashMap<String, Vec<&CN0Observation>> = HashMap::new();
        for obs in observations {
            sat_obs.entry(obs.satellite.clone()).or_insert_with(Vec::new).push(obs);
        }
        
        for (sat_id, obs_list) in sat_obs {
            if obs_list.is_empty() {
                continue;
            }
            
            let cn0_values: Vec<f64> = obs_list.iter().map(|o| o.cn0).collect();
            let elev_values: Vec<f64> = obs_list.iter().map(|o| o.elevation).collect();
            
            let mean_cn0 = cn0_values.iter().sum::<f64>() / cn0_values.len() as f64;
            let min_cn0 = cn0_values.iter().cloned().fold(f64::INFINITY, f64::min);
            let max_cn0 = cn0_values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            
            // Standard deviation
            let variance: f64 = cn0_values.iter().map(|v| (v - mean_cn0).powi(2)).sum::<f64>() / cn0_values.len() as f64;
            let std_cn0 = variance.sqrt();
            
            let mean_elevation = elev_values.iter().sum::<f64>() / elev_values.len() as f64;
            
            stats.insert(sat_id.clone(), SatelliteStats {
                satellite: sat_id,
                system: obs_list[0].system.clone(),
                observation_count: obs_list.len(),
                mean_cn0,
                min_cn0,
                max_cn0,
                std_cn0,
                mean_elevation,
            });
        }
        
        stats
    }
    
    /// Get expected satellite count for a constellation (fallback when no visibility prediction)
    /// Returns typical visible count under good conditions (open sky)
    fn expected_satellites_fallback(system: &str) -> usize {
        match system {
            "GPS" => 12,
            "GLONASS" => 9,
            "Galileo" => 10,
            "BeiDou" => 15,
            "QZSS" => 4,
            "SBAS" => 3,
            "IRNSS" => 7,
            _ => 10,
        }
    }
    
    fn build_constellation_stats(&self, observations: &[CN0Observation], visibility: Option<&VisibilityData>) -> HashMap<String, ConstellationStats> {
        let mut stats: HashMap<String, ConstellationStats> = HashMap::new();
        
        // First, get ALL satellites from RINEX file (not just those with SNR)
        let all_sats_by_system = self.obs_data.satellites_by_system();
        
        // Group SNR observations by system
        let mut sys_obs: HashMap<String, Vec<&CN0Observation>> = HashMap::new();
        for obs in observations {
            sys_obs.entry(obs.system.clone()).or_insert_with(Vec::new).push(obs);
        }
        
        // Build stats for each system that has either RINEX data or SNR observations
        let mut all_systems: Vec<String> = Vec::new();
        for sys_char in all_sats_by_system.keys() {
            let system = match sys_char.as_str() {
                "G" => "GPS",
                "R" => "GLONASS",
                "E" => "Galileo",
                "C" => "BeiDou",
                "J" => "QZSS",
                "S" => "SBAS",
                "I" => "IRNSS",
                _ => sys_char.as_str(),
            }.to_string();
            if !all_systems.contains(&system) {
                all_systems.push(system);
            }
        }
        for system in sys_obs.keys() {
            if !all_systems.contains(system) {
                all_systems.push(system.clone());
            }
        }
        
        for system in all_systems {
            // Get system char for RINEX lookup
            let sys_char = match system.as_str() {
                "GPS" => "G",
                "GLONASS" => "R",
                "Galileo" => "E",
                "BeiDou" => "C",
                "QZSS" => "J",
                "SBAS" => "S",
                "IRNSS" => "I",
                _ => &system,
            }.to_string();
            
            // Count ALL satellites from RINEX (even without SNR)
            let all_sats_in_rinex = all_sats_by_system.get(&sys_char)
                .map(|v| v.len())
                .unwrap_or(0);
            
            // Get SNR observation stats
            let obs_list = sys_obs.get(&system).map(|v| v.as_slice()).unwrap_or(&[]);
            
            if all_sats_in_rinex == 0 && obs_list.is_empty() {
                continue;
            }
            
            // Count unique satellites WITH SNR data
            let mut satellites_with_snr: Vec<String> = Vec::new();
            for obs in obs_list {
                if !satellites_with_snr.contains(&obs.satellite) {
                    satellites_with_snr.push(obs.satellite.clone());
                }
            }
            
            // Use RINEX satellite count (all satellites, not just SNR)
            let satellite_count = all_sats_in_rinex.max(satellites_with_snr.len());
            
            let (mean_cn0, min_cn0, max_cn0, std_cn0) = if !obs_list.is_empty() {
                let cn0_values: Vec<f64> = obs_list.iter().map(|o| o.cn0).collect();
                let mean = cn0_values.iter().sum::<f64>() / cn0_values.len() as f64;
                let min = cn0_values.iter().cloned().fold(f64::INFINITY, f64::min);
                let max = cn0_values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                let variance = cn0_values.iter()
                    .map(|x| (x - mean).powi(2))
                    .sum::<f64>() / cn0_values.len() as f64;
                (mean, min, max, variance.sqrt())
            } else {
                (0.0, 0.0, 0.0, 0.0)
            };
            
            // Get expected satellites from visibility prediction (computed from BRDC ephemeris)
            // Falls back to hardcoded typical values if no navigation data
            // IMPORTANT: Use max(predicted, observed) because nav file may be incomplete
            let satellites_expected = if let Some(vis) = visibility {
                // Get predicted count from visibility assessment
                let predicted = vis.by_system.get(&system)
                    .map(|sys_vis| sys_vis.predicted_satellites)
                    .unwrap_or(0);
                
                // Get observed count from visibility assessment
                let observed = vis.by_system.get(&system)
                    .map(|sys_vis| sys_vis.observed_satellites)
                    .unwrap_or(0);
                
                // Use max(predicted, observed) - if we observed more than predicted,
                // it means nav file was incomplete, not that we have "extra" satellites
                // Only if observed < predicted would this indicate a real issue (jamming)
                let from_visibility = predicted.max(observed);
                
                // If visibility has data, use it; otherwise fallback
                if from_visibility > 0 {
                    from_visibility
                } else {
                    Self::expected_satellites_fallback(&system)
                }
            } else {
                // No visibility data - use fallback
                Self::expected_satellites_fallback(&system)
            };
            
            // Ensure expected is at least 1 to avoid division by zero
            let satellites_expected = satellites_expected.max(1);
            let availability_ratio = satellite_count as f64 / satellites_expected as f64;
            
            // Count cycle slips (CN0 drops > 10 dB between consecutive obs for same satellite)
            let mut cycle_slips = 0usize;
            for sat in &satellites_with_snr {
                let mut sat_cn0: Vec<f64> = obs_list.iter()
                    .filter(|o| &o.satellite == sat)
                    .map(|o| o.cn0)
                    .collect();
                sat_cn0.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                
                for i in 1..sat_cn0.len() {
                    if (sat_cn0[i] - sat_cn0[i-1]).abs() > 10.0 {
                        cycle_slips += 1;
                    }
                }
            }
            
            // Count data gaps (epochs where satellite was expected but missing)
            let mut epoch_sats: HashMap<String, Vec<String>> = HashMap::new();
            for obs in obs_list {
                epoch_sats.entry(obs.timestamp.clone())
                    .or_insert_with(Vec::new)
                    .push(obs.satellite.clone());
            }
            
            let mut data_gaps = 0usize;
            let mut timestamps: Vec<&String> = epoch_sats.keys().collect();
            timestamps.sort();
            
            for sat in &satellites_with_snr {
                let mut last_seen: Option<usize> = None;
                for (idx, ts) in timestamps.iter().enumerate() {
                    let sats_at_epoch = epoch_sats.get(*ts).unwrap();
                    if sats_at_epoch.contains(sat) {
                        if let Some(last) = last_seen {
                            if idx - last > 1 {
                                data_gaps += idx - last - 1;
                            }
                        }
                        last_seen = Some(idx);
                    }
                }
            }
            
            stats.insert(system.clone(), ConstellationStats {
                system,
                satellite_count,
                observation_count: obs_list.len(),
                mean_cn0,
                min_cn0,
                max_cn0,
                std_cn0,
                satellites_expected,
                availability_ratio,
                cycle_slips,
                data_gaps,
            });
        }
        
        stats
    }
    
    fn build_timeseries(&self, observations: &[CN0Observation]) -> TimeseriesData {
        let mut ts = TimeseriesData::default();
        
        // Group by timestamp
        let mut epoch_obs: HashMap<String, Vec<&CN0Observation>> = HashMap::new();
        for obs in observations {
            epoch_obs.entry(obs.timestamp.clone()).or_insert_with(Vec::new).push(obs);
        }
        
        // Sort timestamps
        let mut timestamps: Vec<String> = epoch_obs.keys().cloned().collect();
        timestamps.sort();
        
        // Build epoch-level timeseries
        for timestamp in &timestamps {
            if let Some(obs_list) = epoch_obs.get(timestamp) {
                let mean_cn0 = obs_list.iter().map(|o| o.cn0).sum::<f64>() / obs_list.len() as f64;
                let sat_count = {
                    let mut sats: Vec<&String> = Vec::new();
                    for o in obs_list {
                        if !sats.contains(&&o.satellite) {
                            sats.push(&o.satellite);
                        }
                    }
                    sats.len()
                };
                
                ts.timestamps.push(timestamp.clone());
                ts.mean_cn0.push(mean_cn0);
                ts.satellite_count.push(sat_count);
            }
        }
        
        // Build per-satellite timeseries
        let mut sat_obs: HashMap<String, Vec<&CN0Observation>> = HashMap::new();
        for obs in observations {
            sat_obs.entry(obs.satellite.clone()).or_insert_with(Vec::new).push(obs);
        }
        
        for (sat_id, obs_list) in sat_obs {
            if obs_list.is_empty() {
                continue;
            }
            
            let signal = obs_list[0].signal.clone();
            let system = obs_list[0].system.clone();
            
            let mut cn0_series: Vec<TimeseriesPoint> = Vec::new();
            for obs in obs_list {
                cn0_series.push(TimeseriesPoint {
                    timestamp: obs.timestamp.clone(),
                    cn0: obs.cn0,
                    elevation: obs.elevation,
                    azimuth: obs.azimuth,
                });
            }
            
            // Sort by timestamp
            cn0_series.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
            
            ts.satellite_timeseries.insert(sat_id.clone(), SatelliteTimeseries {
                satellite: sat_id,
                system,
                signal,
                cn0_series,
            });
        }
        
        // Build per-constellation timeseries
        let mut sys_epoch_obs: HashMap<String, HashMap<String, Vec<&CN0Observation>>> = HashMap::new();
        for obs in observations {
            sys_epoch_obs
                .entry(obs.system.clone())
                .or_insert_with(HashMap::new)
                .entry(obs.timestamp.clone())
                .or_insert_with(Vec::new)
                .push(obs);
        }
        
        for (system, epoch_map) in sys_epoch_obs {
            let mut points: Vec<TimeseriesPoint> = Vec::new();
            for (timestamp, obs_list) in epoch_map {
                let mean_cn0 = obs_list.iter().map(|o| o.cn0).sum::<f64>() / obs_list.len() as f64;
                let mean_elev = obs_list.iter().map(|o| o.elevation).sum::<f64>() / obs_list.len() as f64;
                let mean_az = obs_list.iter().map(|o| o.azimuth).sum::<f64>() / obs_list.len() as f64;
                
                points.push(TimeseriesPoint {
                    timestamp,
                    cn0: mean_cn0,
                    elevation: mean_elev,
                    azimuth: mean_az,
                });
            }
            points.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
            ts.constellation_timeseries.insert(system, points);
        }
        
        ts
    }
    
    fn build_skyplot(&self, observations: &[CN0Observation]) -> SkyplotData {
        let mut skyplot = SkyplotData::default();
        
        // Group by satellite
        let mut sat_obs: HashMap<String, Vec<&CN0Observation>> = HashMap::new();
        for obs in observations {
            sat_obs.entry(obs.satellite.clone()).or_insert_with(Vec::new).push(obs);
        }
        
        // Build traces
        for (sat_id, obs_list) in &sat_obs {
            let sys_char = sat_id.chars().next().unwrap_or('G');
            let system = match sys_char {
                'G' => "GPS",
                'R' => "GLONASS",
                'E' => "Galileo",
                'C' => "BeiDou",
                _ => "Other",
            }.to_string();
            
            let mut points: Vec<SkyplotPoint> = Vec::new();
            for obs in obs_list {
                points.push(SkyplotPoint {
                    azimuth: obs.azimuth,
                    elevation: obs.elevation,
                    cn0: obs.cn0,
                    satellite: obs.satellite.clone(),
                });
            }
            
            if !points.is_empty() {
                skyplot.traces.push(SkyplotTrace {
                    satellite: sat_id.clone(),
                    system,
                    points,
                });
            }
        }
        
        // Build heatmap points (bin by azimuth and elevation)
        let az_bin_size = 10.0;
        let el_bin_size = 5.0;
        
        let mut bins: HashMap<(i32, i32), Vec<f64>> = HashMap::new();
        
        for obs in observations {
            let az_bin = (obs.azimuth / az_bin_size).floor() as i32;
            let el_bin = (obs.elevation / el_bin_size).floor() as i32;
            bins.entry((az_bin, el_bin)).or_insert_with(Vec::new).push(obs.cn0);
        }
        
        for ((az_bin, el_bin), cn0_values) in bins {
            let mean_cn0 = cn0_values.iter().sum::<f64>() / cn0_values.len() as f64;
            skyplot.heatmap_points.push(HeatmapPoint {
                azimuth_bin: az_bin as f64 * az_bin_size + az_bin_size / 2.0,
                elevation_bin: el_bin as f64 * el_bin_size + el_bin_size / 2.0,
                mean_cn0,
                count: cn0_values.len(),
            });
        }
        
        // Calculate coverage
        let total_bins = (360.0 / az_bin_size) * (90.0 / el_bin_size);
        let covered_bins = skyplot.heatmap_points.len();
        skyplot.coverage_percent = (covered_bins as f64 / total_bins) * 100.0;
        
        skyplot
    }
    
    fn build_heatmap(&self, observations: &[CN0Observation]) -> HeatmapData {
        let mut heatmap = HeatmapData::default();
        
        let az_bin_size = 10.0;
        let el_bin_size = 5.0;
        let az_bins = 36; // 0-360 in 10° steps
        let el_bins = 18; // 0-90 in 5° steps
        
        // Initialize bins
        heatmap.azimuth_bins = (0..az_bins).map(|i| i as f64 * az_bin_size + az_bin_size / 2.0).collect();
        heatmap.elevation_bins = (0..el_bins).map(|i| i as f64 * el_bin_size + el_bin_size / 2.0).collect();
        
        heatmap.cn0_matrix = vec![vec![0.0; az_bins]; el_bins];
        heatmap.count_matrix = vec![vec![0; az_bins]; el_bins];
        
        // Accumulate values
        let mut sum_matrix: Vec<Vec<f64>> = vec![vec![0.0; az_bins]; el_bins];
        
        for obs in observations {
            let az_idx = ((obs.azimuth / az_bin_size).floor() as usize).min(az_bins - 1);
            let el_idx = ((obs.elevation / el_bin_size).floor() as usize).min(el_bins - 1);
            
            sum_matrix[el_idx][az_idx] += obs.cn0;
            heatmap.count_matrix[el_idx][az_idx] += 1;
        }
        
        // Compute means
        for el_idx in 0..el_bins {
            for az_idx in 0..az_bins {
                let count = heatmap.count_matrix[el_idx][az_idx];
                if count > 0 {
                    heatmap.cn0_matrix[el_idx][az_idx] = sum_matrix[el_idx][az_idx] / count as f64;
                }
            }
        }
        
        heatmap
    }
    
    fn detect_anomalies(&self, observations: &[CN0Observation]) -> Vec<AnomalyEvent> {
        let mut anomalies = Vec::new();
        
        // Build baseline CN0 per satellite (using elevation-dependent model)
        let mut sat_baseline: HashMap<String, Vec<(f64, f64)>> = HashMap::new(); // (elevation, cn0)
        
        for obs in observations {
            sat_baseline.entry(obs.satellite.clone())
                .or_insert_with(Vec::new)
                .push((obs.elevation, obs.cn0));
        }
        
        // Compute expected CN0 vs elevation model
        let mut sat_models: HashMap<String, (f64, f64)> = HashMap::new(); // (mean, std)
        for (sat_id, values) in &sat_baseline {
            if values.len() >= 10 {
                let cn0_values: Vec<f64> = values.iter().map(|(_, c)| *c).collect();
                let mean = cn0_values.iter().sum::<f64>() / cn0_values.len() as f64;
                let variance: f64 = cn0_values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / cn0_values.len() as f64;
                let std = variance.sqrt().max(1.0);
                sat_models.insert(sat_id.clone(), (mean, std));
            }
        }
        
        // Detect CN0 drops
        let mut epoch_obs: HashMap<String, Vec<&CN0Observation>> = HashMap::new();
        for obs in observations {
            epoch_obs.entry(obs.timestamp.clone()).or_insert_with(Vec::new).push(obs);
        }
        
        let mut timestamps: Vec<String> = epoch_obs.keys().cloned().collect();
        timestamps.sort();
        
        for timestamp in &timestamps {
            if let Some(obs_list) = epoch_obs.get(timestamp) {
                let mut affected_satellites = Vec::new();
                let mut affected_systems = Vec::new();
                let mut max_drop = 0.0f64;
                
                for obs in obs_list {
                    if let Some((mean, std)) = sat_models.get(&obs.satellite) {
                        let drop = mean - obs.cn0;
                        
                        // Use a minimum std floor of 5.0 dB to avoid flagging normal variations
                        // Normal CN0 varies significantly with elevation changes
                        let effective_std = std.max(5.0);
                        
                        // Require LARGE absolute drop (10+ dB) AND significant relative drop
                        // This eliminates most normal atmospheric/multipath effects
                        let is_significant = drop > self.config.anomaly_threshold_low * effective_std 
                                            && drop > 10.0;
                        
                        if is_significant {
                            if !affected_satellites.contains(&obs.satellite) {
                                affected_satellites.push(obs.satellite.clone());
                            }
                            if !affected_systems.contains(&obs.system) {
                                affected_systems.push(obs.system.clone());
                            }
                            max_drop = max_drop.max(drop);
                        }
                    }
                }
                
                // Only create an anomaly event if MULTIPLE satellites are affected
                // Single-satellite drops are usually just normal multipath/obstructions
                if affected_satellites.len() >= 2 {
                    let severity = if max_drop >= self.config.anomaly_threshold_critical {
                        "critical"
                    } else if max_drop >= self.config.anomaly_threshold_high {
                        "high"
                    } else {
                        "low"
                    };
                    
                    let event_type = if affected_systems.len() > 1 || affected_satellites.len() > 3 {
                        "Multi-system CN0 drop"
                    } else if affected_satellites.len() > 1 {
                        "Multi-satellite CN0 drop"
                    } else {
                        "Single satellite CN0 drop"
                    };
                    
                    let description = format!(
                        "{:.1} dB-Hz drop affecting {} satellite(s)",
                        max_drop,
                        affected_satellites.len()
                    );
                    
                    anomalies.push(AnomalyEvent {
                        timestamp: timestamp.clone(),
                        severity: severity.to_string(),
                        event_type: event_type.to_string(),
                        description,
                        cn0_drop: max_drop,
                        affected_satellites,
                        affected_systems,
                    });
                }
            }
        }
        
        anomalies
    }
    
    fn compute_quality_score(&self, observations: &[CN0Observation], result: &AnalysisResult) -> QualityScore {
        let mut score = QualityScore {
            overall: 0.0,
            cn0_score: 0.0,
            coverage_score: 0.0,
            consistency_score: 0.0,
            grade: "N/A".to_string(),
            interpretation: "Insufficient data".to_string(),
        };
        
        if observations.is_empty() {
            return score;
        }
        
        // CN0 score (based on mean CN0 relative to expected ~42 dB-Hz)
        let mean_cn0 = result.summary.mean_cn0;
        score.cn0_score = ((mean_cn0 - 30.0) / 15.0 * 100.0).clamp(0.0, 100.0);
        
        // Coverage score (based on skyplot coverage)
        score.coverage_score = result.skyplot.coverage_percent.clamp(0.0, 100.0);
        
        // Consistency score (based on anomaly rate and visibility confirmation)
        let anomaly_rate = result.summary.anomaly_count as f64 / result.summary.total_epochs.max(1) as f64;
        let mut consistency_base = ((1.0 - anomaly_rate * 10.0) * 100.0).clamp(0.0, 100.0);
        
        // Adjust consistency based on visibility (if available)
        if let Some(vis) = &result.visibility {
            // Penalize low confirmation rate
            let vis_penalty = ((100.0 - vis.confirmation_rate) * 0.5).clamp(0.0, 30.0);
            consistency_base -= vis_penalty;
            
            // Penalize unexpected satellites (spoofing indicator)
            let unexpected_penalty = (vis.mean_unexpected * 5.0).clamp(0.0, 20.0);
            consistency_base -= unexpected_penalty;
            
            consistency_base = consistency_base.clamp(0.0, 100.0);
        }
        score.consistency_score = consistency_base;
        
        // Overall score (weighted average)
        score.overall = score.cn0_score * 0.35 + score.coverage_score * 0.25 + score.consistency_score * 0.40;
        
        // Grade
        score.grade = if score.overall >= 90.0 {
            "A".to_string()
        } else if score.overall >= 80.0 {
            "B".to_string()
        } else if score.overall >= 70.0 {
            "C".to_string()
        } else if score.overall >= 60.0 {
            "D".to_string()
        } else {
            "F".to_string()
        };
        
        // Interpretation with visibility info
        // Only add warnings for SIGNIFICANT issues, not minor discrepancies
        let vis_note = if let Some(vis) = &result.visibility {
            // Only warn about spoofing if MANY unexpected satellites (>30% of observed)
            let unexpected_ratio = if vis.mean_observed > 0.0 {
                vis.mean_unexpected / vis.mean_observed
            } else {
                0.0
            };
            
            if unexpected_ratio > 0.30 && vis.mean_unexpected > 5.0 {
                " - WARNING: Many unexpected satellites detected"
            } else if vis.confirmation_rate < 50.0 && vis.mean_predicted > 10.0 {
                // Only warn about jamming if confirmation is VERY low with good ephemeris
                " - WARNING: Low satellite confirmation rate"
            } else {
                ""
            }
        } else {
            ""
        };
        
        score.interpretation = if score.overall >= 90.0 {
            format!("Excellent GNSS signal quality{}", vis_note)
        } else if score.overall >= 80.0 {
            format!("Good GNSS signal quality{}", vis_note)
        } else if score.overall >= 70.0 {
            format!("Fair GNSS signal quality with some issues{}", vis_note)
        } else if score.overall >= 60.0 {
            format!("Poor GNSS signal quality{}", vis_note)
        } else {
            format!("Very poor GNSS signal quality - significant interference suspected{}", vis_note)
        };
        
        score
    }
}
