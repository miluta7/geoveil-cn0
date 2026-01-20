//! Python bindings for geoveil_cn0

#[cfg(feature = "python")]
use pyo3::prelude::*;
#[cfg(feature = "python")]
use pyo3::types::PyModule;
#[cfg(feature = "python")]
use std::collections::HashMap;

#[cfg(feature = "python")]
use crate::types::*;
#[cfg(feature = "python")]
use crate::rinex::parse_rinex_obs;
#[cfg(feature = "python")]
use crate::navigation::{parse_navigation, NavigationData};
#[cfg(feature = "python")]
use crate::cn0::CN0Analyzer;

/// Version string
#[cfg(feature = "python")]
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Python wrapper for QualityScore with notebook-compatible attributes
#[cfg(feature = "python")]
#[pyclass(name = "QualityScore", get_all)]
#[derive(Clone)]
pub struct PyQualityScore {
    pub overall: f64,
    pub rating: String,
    pub cn0_quality: f64,
    pub availability: f64,
    pub continuity: f64,
    pub stability: f64,
    pub diversity: f64,
    pub post_processing_suitable: bool,
}

#[cfg(feature = "python")]
#[pymethods]
impl PyQualityScore {
    fn __repr__(&self) -> String {
        format!("QualityScore(overall={:.0}, rating='{}')", self.overall, self.rating)
    }
}

/// Python wrapper for AnalysisResult
#[cfg(feature = "python")]
#[pyclass(name = "AnalysisResult")]
pub struct PyAnalysisResult {
    inner: AnalysisResult,
}

#[cfg(feature = "python")]
#[pymethods]
impl PyAnalysisResult {
    /// Convert to JSON string
    fn to_json(&self) -> String {
        self.inner.to_json()
    }
    
    /// Convert to pretty JSON string
    fn to_json_pretty(&self) -> String {
        self.inner.to_json_pretty()
    }
    
    // ============ File Info ============
    
    /// Get filename
    #[getter]
    fn filename(&self) -> String {
        self.inner.file_info.filename.clone()
    }
    
    /// Get RINEX version (placeholder)
    #[getter]
    fn rinex_version(&self) -> String {
        "3.05".to_string()  // TODO: extract from RINEX header
    }
    
    /// Get station name (marker name)
    #[getter]
    fn station_name(&self) -> Option<String> {
        let name = &self.inner.file_info.marker_name;
        if name.is_empty() {
            None
        } else {
            Some(name.clone())
        }
    }
    
    /// Get receiver type
    #[getter]
    fn receiver_type(&self) -> String {
        self.inner.file_info.receiver_type.clone()
    }
    
    /// Get antenna type
    #[getter]
    fn antenna_type(&self) -> String {
        self.inner.file_info.antenna_type.clone()
    }
    
    /// Get start time
    #[getter]
    fn start_time(&self) -> String {
        self.inner.file_info.start_time.clone()
    }
    
    /// Get end time
    #[getter]
    fn end_time(&self) -> String {
        self.inner.file_info.end_time.clone()
    }
    
    /// Get duration in hours
    #[getter]
    fn duration_hours(&self) -> f64 {
        self.inner.file_info.duration_hours
    }
    
    /// Get observation interval
    #[getter]
    fn interval(&self) -> f64 {
        self.inner.file_info.interval
    }
    
    // ============ Summary Statistics ============
    
    /// Get total observations
    #[getter]
    fn total_observations(&self) -> usize {
        self.inner.summary.total_observations
    }
    
    /// Get total satellites
    #[getter]
    fn total_satellites(&self) -> usize {
        self.inner.summary.total_satellites
    }
    
    /// Get total epochs (alias: epoch_count)
    #[getter]
    fn total_epochs(&self) -> usize {
        self.inner.summary.total_epochs
    }
    
    /// Get epoch count (alias for total_epochs)
    #[getter]
    fn epoch_count(&self) -> usize {
        self.inner.summary.total_epochs
    }
    
    /// Get constellations observed
    #[getter]
    fn constellations(&self) -> Vec<String> {
        self.inner.summary.systems_observed.clone()
    }
    
    /// Get mean CN0 (alias: avg_cn0)
    #[getter]
    fn mean_cn0(&self) -> f64 {
        self.inner.summary.mean_cn0
    }
    
    /// Get average CN0 (alias for mean_cn0)
    #[getter]
    fn avg_cn0(&self) -> f64 {
        self.inner.summary.mean_cn0
    }
    
    /// Get min CN0
    #[getter]
    fn min_cn0(&self) -> f64 {
        self.inner.summary.min_cn0
    }
    
    /// Get max CN0
    #[getter]
    fn max_cn0(&self) -> f64 {
        self.inner.summary.max_cn0
    }
    
    /// Get CN0 standard deviation (computed from timeseries)
    #[getter]
    fn cn0_std_dev(&self) -> f64 {
        let values = &self.inner.timeseries.mean_cn0;
        if values.is_empty() {
            return 0.0;
        }
        let mean = values.iter().sum::<f64>() / values.len() as f64;
        let variance = values.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / values.len() as f64;
        variance.sqrt()
    }
    
    // ============ Anomaly Detection ============
    
    /// Get anomaly count
    #[getter]
    fn anomaly_count(&self) -> usize {
        self.inner.summary.anomaly_count
    }
    
    /// Get critical anomaly count
    #[getter]
    fn critical_count(&self) -> usize {
        self.inner.summary.critical_count
    }
    
    /// Check if jamming detected
    /// Real jamming causes: severe CN0 drops (>10dB) across MULTIPLE satellites simultaneously
    /// AND low average CN0 (<35 dB-Hz) or high percentage of critical anomalies
    #[getter]
    fn jamming_detected(&self) -> bool {
        // Jamming evidence requires multiple factors:
        // 1. Low average CN0 (suggests wideband interference)
        let low_cn0 = self.inner.summary.mean_cn0 < 35.0;
        
        // 2. High ratio of critical anomalies (>5% of epochs)
        let epoch_count = self.inner.timeseries.timestamps.len().max(1);
        let critical_ratio = self.inner.summary.critical_count as f64 / epoch_count as f64;
        let many_critical = critical_ratio > 0.05;
        
        // 3. Multi-system critical anomalies (jamming affects all constellations)
        let multi_system_critical = self.inner.anomalies.iter()
            .filter(|a| a.severity == "critical")
            .any(|a| a.affected_systems.len() > 1);
        
        // Require: (low CN0 AND many critical) OR (multi-system critical AND many critical)
        (low_cn0 && many_critical) || (multi_system_critical && many_critical)
    }
    
    /// Check if spoofing detected
    /// Real spoofing indicators:
    /// 1. Many unexpected satellites (>40% not in ephemeris) with GOOD signals
    /// 2. Satellites appearing with impossible geometry (wrong hemisphere, etc.)
    /// Note: Low CN0 std deviation is NOT a spoofing indicator - it's actually good data!
    #[getter]
    fn spoofing_detected(&self) -> bool {
        // Check for unexpected satellites (only meaningful with ephemeris data)
        if let Some(vis) = &self.inner.visibility {
            let total_observed = vis.mean_observed;
            let unexpected = vis.mean_unexpected;
            
            // Need significant unexpected satellites (>40%) AND more than 8 total
            // AND high confidence - this is a very serious accusation!
            let unexpected_ratio = if total_observed > 0.0 {
                unexpected / total_observed
            } else {
                0.0
            };
            
            // Very strict threshold - false positives for spoofing are very bad
            let many_unexpected = unexpected_ratio > 0.40 && unexpected > 8.0;
            
            if many_unexpected {
                return true;
            }
        }
        
        // Note: We REMOVED the "uniform CN0" check because:
        // - Low std deviation over time indicates STABLE signals (good!)
        // - Real spoofing detection requires cross-checking satellite geometry,
        //   clock consistency, and other sophisticated methods
        // - False positives for spoofing are extremely damaging
        
        false
    }
    
    /// Check if interference detected
    /// Requires meaningful anomaly count - not just isolated events
    #[getter]
    fn interference_detected(&self) -> bool {
        // Require at least 3 anomalies AND they must affect >1% of epochs
        let epoch_count = self.inner.timeseries.timestamps.len().max(1);
        let anomaly_ratio = self.inner.summary.anomaly_count as f64 / epoch_count as f64;
        
        // Need at least 3 anomalies AND >1% of epochs affected
        self.inner.summary.anomaly_count >= 3 && anomaly_ratio > 0.01
    }
    
    /// Get anomalies list
    fn get_anomalies(&self) -> Vec<HashMap<String, String>> {
        self.inner.anomalies.iter().map(|a| {
            let mut m = HashMap::new();
            m.insert("start_time".to_string(), a.timestamp.clone());
            m.insert("end_time".to_string(), a.timestamp.clone());  // Same as start (single point)
            m.insert("severity".to_string(), a.severity.clone());
            m.insert("anomaly_type".to_string(), a.event_type.clone());
            m.insert("description".to_string(), a.description.clone());
            m.insert("cn0_drop".to_string(), a.cn0_drop.to_string());
            m.insert("affected_satellites".to_string(), a.affected_satellites.join(","));
            m.insert("affected_systems".to_string(), a.affected_systems.join(","));
            m
        }).collect()
    }
    
    // ============ Quality Score ============
    
    /// Get quality score object with all metrics
    #[getter]
    fn quality_score(&self) -> PyQualityScore {
        let qs = &self.inner.quality_score;
        
        // Compute proper AVAILABILITY from constellation stats
        // This is satellites observed / expected, capped at 100%
        let availability = if self.inner.constellation_stats.is_empty() {
            50.0 // Default if no data
        } else {
            let total_observed: usize = self.inner.constellation_stats.values()
                .map(|cs| cs.satellite_count)
                .sum();
            let total_expected: usize = self.inner.constellation_stats.values()
                .map(|cs| cs.satellites_expected)
                .sum();
            if total_expected > 0 {
                ((total_observed as f64 / total_expected as f64) * 100.0).min(100.0)
            } else {
                50.0
            }
        };
        
        // Compute STABILITY from CN0 standard deviation
        // Lower std dev = more stable signal = higher score
        // Typical good: 3-5 dB-Hz std, poor: >8 dB-Hz std
        let cn0_values: Vec<f64> = self.inner.constellation_stats.values()
            .map(|cs| cs.std_cn0)
            .collect();
        let avg_std = if cn0_values.is_empty() {
            5.0 // Default
        } else {
            cn0_values.iter().sum::<f64>() / cn0_values.len() as f64
        };
        // Score: 100 at 2dB std, 0 at 10dB std
        let stability = ((10.0 - avg_std) / 8.0 * 100.0).clamp(0.0, 100.0);
        
        // CONTINUITY: based on data gaps and cycle slips (lower = better)
        let total_gaps: usize = self.inner.constellation_stats.values()
            .map(|cs| cs.data_gaps)
            .sum();
        let total_epochs = self.inner.summary.total_epochs.max(1);
        // Score: 100 if no gaps, decreases with more gaps
        let gap_ratio = total_gaps as f64 / (total_epochs as f64 * self.inner.constellation_stats.len().max(1) as f64);
        let continuity = ((1.0 - gap_ratio.min(1.0)) * 100.0).clamp(0.0, 100.0);
        
        // DIVERSITY: number of systems observed (4 systems = 100%)
        let diversity = (self.inner.summary.systems_observed.len() as f64 / 4.0 * 100.0).min(100.0);
        
        // CN0 QUALITY: from the computed cn0_score
        let cn0_quality = qs.cn0_score;
        
        // OVERALL: weighted average
        let overall = cn0_quality * 0.30 + availability * 0.25 + continuity * 0.20 + stability * 0.15 + diversity * 0.10;
        
        // Rating
        let rating = if overall >= 90.0 {
            "A - Excellent".to_string()
        } else if overall >= 80.0 {
            "B - Good".to_string()
        } else if overall >= 70.0 {
            "C - Fair".to_string()
        } else if overall >= 60.0 {
            "D - Poor".to_string()
        } else {
            "F - Very Poor".to_string()
        };
        
        PyQualityScore {
            overall,
            rating,
            cn0_quality,
            availability,
            continuity,
            stability,
            diversity,
            post_processing_suitable: overall >= 65.0 && cn0_quality >= 60.0,
        }
    }
    
    /// Get quality score value (alias for overall)
    #[getter]
    fn score(&self) -> f64 {
        self.inner.quality_score.overall
    }
    
    /// Get quality grade
    #[getter]
    fn quality_grade(&self) -> String {
        self.inner.quality_score.grade.clone()
    }
    
    /// Get summary interpretation
    #[getter]
    fn summary(&self) -> String {
        self.inner.quality_score.interpretation.clone()
    }
    
    // ============ Constellation Data ============
    
    /// Get constellation summary by name
    fn get_constellation_summary(&self, name: &str) -> Option<HashMap<String, String>> {
        self.inner.constellation_stats.get(name).map(|cs| {
            let mut m = HashMap::new();
            m.insert("system".to_string(), cs.system.clone());
            m.insert("constellation".to_string(), cs.system.clone()); // Alias for notebook
            m.insert("satellite_count".to_string(), cs.satellite_count.to_string());
            m.insert("satellites_observed".to_string(), cs.satellite_count.to_string()); // Alias
            m.insert("satellites_expected".to_string(), cs.satellites_expected.to_string());
            m.insert("observation_count".to_string(), cs.observation_count.to_string());
            m.insert("mean_cn0".to_string(), format!("{:.1}", cs.mean_cn0));
            m.insert("cn0_mean".to_string(), format!("{:.1}", cs.mean_cn0)); // Alias
            m.insert("min_cn0".to_string(), format!("{:.1}", cs.min_cn0));
            m.insert("max_cn0".to_string(), format!("{:.1}", cs.max_cn0));
            m.insert("std_cn0".to_string(), format!("{:.2}", cs.std_cn0));
            m.insert("cn0_std".to_string(), format!("{:.2}", cs.std_cn0)); // Alias
            m.insert("availability_ratio".to_string(), format!("{:.3}", cs.availability_ratio));
            m.insert("cycle_slips".to_string(), cs.cycle_slips.to_string());
            m.insert("data_gaps".to_string(), cs.data_gaps.to_string());
            m
        })
    }
    
    /// Get systems observed
    fn get_systems(&self) -> Vec<String> {
        self.inner.summary.systems_observed.clone()
    }
    
    // ============ Timeseries Data ============
    
    /// Get timeseries data as dict
    fn get_timeseries_data(&self) -> HashMap<String, Vec<f64>> {
        let mut data = HashMap::new();
        
        // Convert timestamps to hours since start
        let hours: Vec<f64> = (0..self.inner.timeseries.timestamps.len())
            .map(|i| i as f64 * self.inner.file_info.interval / 3600.0)
            .collect();
        data.insert("hours".to_string(), hours);
        data.insert("mean_cn0".to_string(), self.inner.timeseries.mean_cn0.clone());
        data.insert("satellite_count".to_string(), 
            self.inner.timeseries.satellite_count.iter().map(|x| *x as f64).collect());
        
        data
    }
    
    /// Get timeseries timestamps
    fn get_timestamps(&self) -> Vec<String> {
        self.inner.timeseries.timestamps.clone()
    }
    
    /// Get timeseries mean CN0 values
    fn get_timeseries_cn0(&self) -> Vec<f64> {
        self.inner.timeseries.mean_cn0.clone()
    }
    
    // ============ Skyplot Data ============
    
    /// Get skyplot data as list of trace dicts
    fn get_skyplot_data(&self) -> Vec<HashMap<String, String>> {
        self.inner.skyplot.traces.iter().map(|t| {
            let mut m = HashMap::new();
            m.insert("satellite".to_string(), t.satellite.clone());
            m.insert("system".to_string(), t.system.clone());
            m.insert("azimuths".to_string(), 
                t.points.iter().map(|p| format!("{:.1}", p.azimuth)).collect::<Vec<_>>().join(","));
            m.insert("elevations".to_string(), 
                t.points.iter().map(|p| format!("{:.1}", p.elevation)).collect::<Vec<_>>().join(","));
            m.insert("cn0_values".to_string(), 
                t.points.iter().map(|p| format!("{:.1}", p.cn0)).collect::<Vec<_>>().join(","));
            m
        }).collect()
    }
    
    /// Get skyplot coverage percent
    #[getter]
    fn skyplot_coverage(&self) -> f64 {
        self.inner.skyplot.coverage_percent
    }
    
    /// Get number of skyplot traces
    #[getter]
    fn skyplot_trace_count(&self) -> usize {
        self.inner.skyplot.traces.len()
    }
    
    // ============ Visibility Data ============
    
    /// Get visibility prediction source
    #[getter]
    fn visibility_prediction_source(&self) -> String {
        self.inner.visibility.as_ref()
            .map(|v| v.prediction_source.clone())
            .unwrap_or_else(|| "None".to_string())
    }
    
    /// Get visibility confirmation rate
    #[getter]
    fn visibility_confirmation_rate(&self) -> f64 {
        self.inner.visibility.as_ref()
            .map(|v| v.confirmation_rate)
            .unwrap_or(0.0)
    }
    
    /// Get mean predicted satellites
    #[getter]
    fn visibility_mean_predicted(&self) -> f64 {
        self.inner.visibility.as_ref().map(|v| v.mean_predicted).unwrap_or(0.0)
    }
    
    /// Get mean observed satellites
    #[getter]
    fn visibility_mean_observed(&self) -> f64 {
        self.inner.visibility.as_ref().map(|v| v.mean_observed).unwrap_or(0.0)
    }
    
    /// Get mean missing satellites per epoch
    #[getter]
    fn visibility_mean_missing(&self) -> f64 {
        self.inner.visibility.as_ref().map(|v| v.mean_missing).unwrap_or(0.0)
    }
    
    /// Get mean unexpected satellites per epoch (seen but not predicted)
    #[getter]
    fn visibility_mean_unexpected(&self) -> f64 {
        self.inner.visibility.as_ref().map(|v| v.mean_unexpected).unwrap_or(0.0)
    }
    
    /// Get frequently missing satellites (possible obstructions)
    fn get_frequently_missing_satellites(&self) -> Vec<String> {
        self.inner.visibility.as_ref()
            .map(|v| v.frequently_missing.clone())
            .unwrap_or_default()
    }
    
    /// Get frequently unexpected satellites (possible spoofing)
    fn get_frequently_unexpected_satellites(&self) -> Vec<String> {
        self.inner.visibility.as_ref()
            .map(|v| v.frequently_unexpected.clone())
            .unwrap_or_default()
    }
    
    /// Check if visibility prediction is available
    #[getter]
    fn has_visibility_prediction(&self) -> bool {
        self.inner.visibility.as_ref().map(|v| v.has_prediction).unwrap_or(false)
    }
    
    /// Get visibility anomaly count
    #[getter]
    fn visibility_anomaly_count(&self) -> usize {
        self.inner.visibility.as_ref()
            .map(|v| v.anomalies.len())
            .unwrap_or(0)
    }
    
    /// Get visibility anomaly types
    fn get_visibility_anomaly_types(&self) -> Vec<String> {
        self.inner.visibility.as_ref()
            .map(|v| v.anomalies.iter().map(|a| a.anomaly_type.clone()).collect())
            .unwrap_or_default()
    }
    
    /// Get visibility anomaly descriptions
    fn get_visibility_anomaly_descriptions(&self) -> Vec<String> {
        self.inner.visibility.as_ref()
            .map(|v| v.anomalies.iter().map(|a| a.description.clone()).collect())
            .unwrap_or_default()
    }
    
    /// Get detailed visibility debug info per system
    fn visibility_debug(&self) -> HashMap<String, HashMap<String, String>> {
        let mut result: HashMap<String, HashMap<String, String>> = HashMap::new();
        
        if let Some(vis) = &self.inner.visibility {
            for (sys_name, sys_stats) in &vis.by_system {
                let mut info: HashMap<String, String> = HashMap::new();
                info.insert("predicted".to_string(), sys_stats.predicted_satellites.to_string());
                info.insert("observed".to_string(), sys_stats.observed_satellites.to_string());
                info.insert("confirmation_rate".to_string(), format!("{:.1}%", sys_stats.confirmation_rate));
                info.insert("missing_count".to_string(), sys_stats.missing_satellites.len().to_string());
                info.insert("unexpected_count".to_string(), sys_stats.unexpected_satellites.len().to_string());
                info.insert("missing_sats".to_string(), sys_stats.missing_satellites.join(", "));
                info.insert("unexpected_sats".to_string(), sys_stats.unexpected_satellites.join(", "));
                result.insert(sys_name.clone(), info);
            }
            
            // Add overall info
            let mut overall: HashMap<String, String> = HashMap::new();
            overall.insert("source".to_string(), vis.prediction_source.clone());
            overall.insert("mean_predicted".to_string(), format!("{:.1}", vis.mean_predicted));
            overall.insert("mean_observed".to_string(), format!("{:.1}", vis.mean_observed));
            overall.insert("mean_missing".to_string(), format!("{:.1}", vis.mean_missing));
            overall.insert("mean_unexpected".to_string(), format!("{:.1}", vis.mean_unexpected));
            overall.insert("confirmation_rate".to_string(), format!("{:.1}%", vis.confirmation_rate));
            result.insert("_overall".to_string(), overall);
        }
        
        result
    }
    
    /// Get timeseries mean CN0 values
    fn get_mean_cn0_series(&self) -> Vec<f64> {
        self.inner.timeseries.mean_cn0.clone()
    }
    
    /// Get timeseries satellite counts
    fn get_satellite_count_series(&self) -> Vec<usize> {
        self.inner.timeseries.satellite_count.clone()
    }
    
    /// Get satellite IDs
    fn get_satellite_ids(&self) -> Vec<String> {
        self.inner.satellite_stats.keys().cloned().collect()
    }
    
    /// Get anomaly timestamps
    fn get_anomaly_timestamps(&self) -> Vec<String> {
        self.inner.anomalies.iter().map(|a| a.timestamp.clone()).collect()
    }
    
    /// Get anomaly severities
    fn get_anomaly_severities(&self) -> Vec<String> {
        self.inner.anomalies.iter().map(|a| a.severity.clone()).collect()
    }
    
    /// Get anomaly CN0 drops
    fn get_anomaly_cn0_drops(&self) -> Vec<f64> {
        self.inner.anomalies.iter().map(|a| a.cn0_drop).collect()
    }
    
    /// Get receiver latitude
    #[getter]
    fn position_lat(&self) -> f64 {
        self.inner.file_info.position_lat
    }
    
    /// Get receiver longitude
    #[getter]
    fn position_lon(&self) -> f64 {
        self.inner.file_info.position_lon
    }
    
    /// String representation
    fn __repr__(&self) -> String {
        format!(
            "AnalysisResult(observations={}, satellites={}, epochs={}, mean_cn0={:.1}, anomalies={})",
            self.inner.summary.total_observations,
            self.inner.summary.total_satellites,
            self.inner.summary.total_epochs,
            self.inner.summary.mean_cn0,
            self.inner.summary.anomaly_count
        )
    }
}

/// Python wrapper for NavigationData
#[cfg(feature = "python")]
#[pyclass(name = "NavigationData")]
pub struct PyNavigationData {
    inner: NavigationData,
}

#[cfg(feature = "python")]
#[pymethods]
impl PyNavigationData {
    /// Check if navigation data is available
    fn has_data(&self) -> bool {
        self.inner.has_data()
    }
    
    /// Get satellite count
    #[getter]
    fn satellite_count(&self) -> usize {
        self.inner.satellite_count()
    }
    
    /// Get GPS satellite count
    #[getter]
    fn gps_count(&self) -> usize {
        self.inner.gps_ephemeris.len()
    }
    
    /// Get GLONASS satellite count
    #[getter]
    fn glonass_count(&self) -> usize {
        self.inner.glonass_ephemeris.len()
    }
    
    /// Get Galileo satellite count
    #[getter]
    fn galileo_count(&self) -> usize {
        self.inner.galileo_ephemeris.len()
    }
    
    /// Get BeiDou satellite count
    #[getter]
    fn beidou_count(&self) -> usize {
        self.inner.beidou_ephemeris.len()
    }
    
    fn __repr__(&self) -> String {
        format!(
            "NavigationData(GPS={}, GLONASS={}, Galileo={}, BeiDou={})",
            self.inner.gps_ephemeris.len(),
            self.inner.glonass_ephemeris.len(),
            self.inner.galileo_ephemeris.len(),
            self.inner.beidou_ephemeris.len()
        )
    }
}

/// Python wrapper for RinexObsData
#[cfg(feature = "python")]
#[pyclass(name = "RinexObsData")]
pub struct PyRinexObsData {
    inner: RinexObsData,
}

#[cfg(feature = "python")]
#[pymethods]
impl PyRinexObsData {
    /// Get number of epochs
    #[getter]
    fn num_epochs(&self) -> usize {
        self.inner.num_epochs()
    }
    
    /// Get number of satellites
    #[getter]
    fn num_satellites(&self) -> usize {
        self.inner.num_satellites()
    }
    
    /// Get RINEX version
    #[getter]
    fn version(&self) -> f64 {
        self.inner.version
    }
    
    /// Get marker name
    #[getter]
    fn marker_name(&self) -> String {
        self.inner.marker_name.clone()
    }
    
    /// Get receiver type
    #[getter]
    fn receiver_type(&self) -> String {
        self.inner.receiver_type.clone()
    }
    
    /// Get interval
    #[getter]
    fn interval(&self) -> f64 {
        self.inner.interval
    }
    
    /// Get satellites grouped by constellation system
    /// Returns dict like {'G': ['G01', 'G02', ...], 'R': ['R01', ...], ...}
    fn satellites_by_system(&self) -> HashMap<String, Vec<String>> {
        self.inner.satellites_by_system()
    }
    
    /// Get observation types by constellation system
    /// Returns dict like {'G': ['C1C', 'L1C', 'S1C', ...], 'C': ['C1I', 'L1I', 'S1I', ...], ...}
    fn obs_types_by_system(&self) -> HashMap<String, Vec<String>> {
        self.inner.obs_types.clone()
    }
    
    /// Check if S (SNR) observations exist for each system
    fn snr_availability(&self) -> HashMap<String, bool> {
        let mut result = HashMap::new();
        for (sys, types) in &self.inner.obs_types {
            let has_snr = types.iter().any(|t| t.starts_with('S'));
            result.insert(sys.clone(), has_snr);
        }
        result
    }
    
    /// Get detailed debug info about parsing
    fn debug_info(&self) -> HashMap<String, String> {
        let mut info = HashMap::new();
        info.insert("version".to_string(), format!("{:.2}", self.inner.version));
        info.insert("num_epochs".to_string(), self.inner.num_epochs().to_string());
        info.insert("num_satellites".to_string(), self.inner.num_satellites().to_string());
        
        // Count satellites per system
        let sats_by_sys = self.inner.satellites_by_system();
        for (sys, sats) in &sats_by_sys {
            info.insert(format!("{}_satellites", sys), sats.len().to_string());
            info.insert(format!("{}_sat_list", sys), sats.join(","));
        }
        
        // Count obs types per system
        for (sys, types) in &self.inner.obs_types {
            info.insert(format!("{}_obs_types_count", sys), types.len().to_string());
            info.insert(format!("{}_obs_types", sys), types.join(","));
            let snr_types: Vec<&String> = types.iter().filter(|t| t.starts_with('S')).collect();
            info.insert(format!("{}_snr_types", sys), snr_types.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(","));
        }
        
        info
    }
    
    /// Get SNR statistics per satellite
    /// Returns HashMap with satellite ID as key, stats dict as value
    fn snr_stats_by_satellite(&self) -> HashMap<String, HashMap<String, f64>> {
        let mut result: HashMap<String, HashMap<String, f64>> = HashMap::new();
        
        // Count SNR observations per satellite
        let mut sat_snr_values: HashMap<String, Vec<f64>> = HashMap::new();
        let mut sat_obs_count: HashMap<String, usize> = HashMap::new();
        
        for epoch in &self.inner.epochs {
            for (sat_id, sat_obs) in &epoch.observations {
                for (obs_code, obs_val) in &sat_obs.values {
                    if obs_code.starts_with('S') {
                        // Count all observations
                        *sat_obs_count.entry(sat_id.clone()).or_insert(0) += 1;
                        
                        // Track valid (non-zero) SNR values
                        if obs_val.value > 0.0 && obs_val.value < 100.0 {
                            sat_snr_values.entry(sat_id.clone())
                                .or_insert_with(Vec::new)
                                .push(obs_val.value);
                        }
                    }
                }
            }
        }
        
        // Build result - use all satellites we've seen
        let all_sats: Vec<String> = sat_obs_count.keys().cloned().collect();
        
        for sat_id in all_sats {
            let mut stats: HashMap<String, f64> = HashMap::new();
            let obs_count = *sat_obs_count.get(&sat_id).unwrap_or(&0) as f64;
            let valid_values = sat_snr_values.get(&sat_id);
            
            stats.insert("obs_count".to_string(), obs_count);
            
            if let Some(values) = valid_values {
                let valid_count = values.len() as f64;
                let mean_snr = if values.is_empty() { 
                    0.0 
                } else { 
                    values.iter().sum::<f64>() / valid_count 
                };
                stats.insert("valid_count".to_string(), valid_count);
                stats.insert("mean_snr".to_string(), mean_snr);
            } else {
                stats.insert("valid_count".to_string(), 0.0);
                stats.insert("mean_snr".to_string(), 0.0);
            }
            
            result.insert(sat_id, stats);
        }
        
        result
    }
    
    fn __repr__(&self) -> String {
        format!(
            "RinexObsData(version={:.2}, epochs={}, satellites={})",
            self.inner.version,
            self.inner.num_epochs(),
            self.inner.num_satellites()
        )
    }
}

// ============ Analysis Config ============

/// Python wrapper for AnalysisConfig
#[cfg(feature = "python")]
#[pyclass(name = "AnalysisConfig", get_all)]
#[derive(Clone)]
pub struct PyAnalysisConfig {
    pub min_elevation: f64,
    pub min_cn0: f64,
    pub max_cn0: f64,
    pub time_bin_seconds: u32,
    pub systems: Option<Vec<String>>,
    pub anomaly_threshold_low: f64,
    pub anomaly_threshold_high: f64,
    pub anomaly_threshold_critical: f64,
    pub detect_anomalies: bool,
    pub anomaly_sensitivity: f64,
    pub interference_threshold_db: f64,
    pub verbose: bool,
    pub nav_file: Option<String>,
}

#[cfg(feature = "python")]
#[pymethods]
impl PyAnalysisConfig {
    #[new]
    #[pyo3(signature = (
        min_elevation = 5.0,
        min_cn0 = 0.0,
        max_cn0 = 60.0,
        time_bin_seconds = None,
        time_bin = None,
        systems = None,
        anomaly_threshold_low = 3.0,
        anomaly_threshold_high = 6.0,
        anomaly_threshold_critical = 10.0,
        detect_anomalies = true,
        anomaly_sensitivity = 0.5,
        interference_threshold_db = 6.0,
        verbose = false,
        nav_file = None
    ))]
    fn new(
        min_elevation: f64,
        min_cn0: f64,
        max_cn0: f64,
        time_bin_seconds: Option<u32>,
        time_bin: Option<u32>,
        systems: Option<Vec<String>>,
        anomaly_threshold_low: f64,
        anomaly_threshold_high: f64,
        #[allow(unused_variables)]
        anomaly_threshold_critical: f64,
        detect_anomalies: bool,
        anomaly_sensitivity: f64,
        interference_threshold_db: f64,
        verbose: bool,
        nav_file: Option<String>,
    ) -> Self {
        let time_bin_seconds = time_bin_seconds.or(time_bin).unwrap_or(60);
        
        // Convert sensitivity (0-1) to thresholds
        // Higher sensitivity = lower thresholds = more detections
        let anomaly_threshold_low = anomaly_threshold_low * (1.0 - anomaly_sensitivity * 0.5);
        let anomaly_threshold_high = anomaly_threshold_high * (1.0 - anomaly_sensitivity * 0.5);
        // Use interference_threshold_db as anomaly_threshold_critical
        let anomaly_threshold_critical = interference_threshold_db;
        
        Self {
            min_elevation,
            min_cn0,
            max_cn0,
            time_bin_seconds,
            systems,
            anomaly_threshold_low,
            anomaly_threshold_high,
            anomaly_threshold_critical,
            detect_anomalies,
            anomaly_sensitivity,
            interference_threshold_db,
            verbose,
            nav_file,
        }
    }
}

// ============ CN0 Analyzer ============

/// Python wrapper for CN0Analyzer - supports both config-based and direct API
#[cfg(feature = "python")]
#[pyclass(name = "CN0Analyzer")]
pub struct PyCN0Analyzer {
    config: PyAnalysisConfig,
    obs_data: Option<RinexObsData>,
    nav_data: Option<NavigationData>,
}

#[cfg(feature = "python")]
#[pymethods]
impl PyCN0Analyzer {
    /// Create new analyzer from config (primary API)
    #[new]
    #[pyo3(signature = (config))]
    fn new(config: &PyAnalysisConfig) -> Self {
        Self {
            config: config.clone(),
            obs_data: None,
            nav_data: None,
        }
    }
    
    /// Alternative constructor from observation data (legacy API)
    #[staticmethod]
    #[pyo3(signature = (obs_data, min_elevation=5.0, time_bin_seconds=60, systems=None))]
    fn from_obs(
        obs_data: &PyRinexObsData,
        min_elevation: f64,
        time_bin_seconds: u64,
        systems: Option<Vec<String>>,
    ) -> Self {
        let systems_vec = systems
            .map(|s| s.iter().filter_map(|c| c.chars().next()).collect::<Vec<char>>())
            .unwrap_or_else(|| vec!['G', 'R', 'E', 'C']);
        let systems_str: Vec<String> = systems_vec.iter().map(|c| c.to_string()).collect();
        
        let config = PyAnalysisConfig {
            min_elevation,
            min_cn0: 0.0,
            max_cn0: 60.0,
            time_bin_seconds: time_bin_seconds as u32,
            systems: Some(systems_str),
            anomaly_threshold_low: 3.0,
            anomaly_threshold_high: 6.0,
            anomaly_threshold_critical: 10.0,
            detect_anomalies: true,
            anomaly_sensitivity: 0.5,
            interference_threshold_db: 6.0,
            verbose: false,
            nav_file: None,
        };
        
        Self {
            config,
            obs_data: Some(obs_data.inner.clone()),
            nav_data: None,
        }
    }
    
    /// Set navigation data for satellite position computation (old API)
    fn set_navigation(&mut self, nav_data: &PyNavigationData) {
        self.nav_data = Some(nav_data.inner.clone());
    }
    
    /// Run CN0 analysis (old API - requires obs_data set via from_obs)
    fn analyze(&self) -> PyResult<PyAnalysisResult> {
        let obs_data = self.obs_data.clone().ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err(
                "No observation data. Use analyze_file() or CN0Analyzer.from_obs()"
            )
        })?;
        
        let systems: Vec<char> = self.config.systems.clone()
            .unwrap_or_else(|| vec!["G".to_string(), "R".to_string(), "E".to_string(), "C".to_string()])
            .iter()
            .filter_map(|s| s.chars().next())
            .collect();
        
        let config = AnalysisConfig {
            min_elevation: self.config.min_elevation,
            min_cn0: self.config.min_cn0,
            max_cn0: self.config.max_cn0,
            time_bin_seconds: self.config.time_bin_seconds as u64,
            systems,
            anomaly_threshold_low: self.config.anomaly_threshold_low,
            anomaly_threshold_high: self.config.anomaly_threshold_high,
            anomaly_threshold_critical: self.config.anomaly_threshold_critical,
        };
        
        let mut analyzer = CN0Analyzer::new(obs_data, config);
        
        if let Some(nav) = &self.nav_data {
            analyzer = analyzer.with_navigation(nav.clone());
        }
        
        let result = analyzer.analyze();
        Ok(PyAnalysisResult { inner: result })
    }
    
    /// Analyze RINEX observation file from path (new API)
    fn analyze_file(&self, obs_path: &str) -> PyResult<PyAnalysisResult> {
        let content = std::fs::read(obs_path)
            .map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))?;
        
        let filename = std::path::Path::new(obs_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown.obs");
        
        let obs_data = parse_rinex_obs(&content, filename)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e))?;
        
        let systems: Vec<char> = self.config.systems.clone()
            .unwrap_or_else(|| vec!["G".to_string(), "R".to_string(), "E".to_string(), "C".to_string()])
            .iter()
            .filter_map(|s| s.chars().next())
            .collect();
        
        let config = AnalysisConfig {
            min_elevation: self.config.min_elevation,
            min_cn0: self.config.min_cn0,
            max_cn0: self.config.max_cn0,
            time_bin_seconds: self.config.time_bin_seconds as u64,
            systems,
            anomaly_threshold_low: self.config.anomaly_threshold_low,
            anomaly_threshold_high: self.config.anomaly_threshold_high,
            anomaly_threshold_critical: self.config.anomaly_threshold_critical,
        };
        
        // Check if nav_file is set in config
        let nav_data = if let Some(nav_path) = &self.config.nav_file {
            let nav_content = std::fs::read(nav_path)
                .map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))?;
            Some(parse_navigation(&nav_content)
                .map_err(|e| pyo3::exceptions::PyValueError::new_err(e))?)
        } else {
            None
        };
        
        let mut analyzer = CN0Analyzer::new(obs_data, config);
        
        if let Some(nav) = nav_data {
            analyzer = analyzer.with_navigation(nav);
        }
        
        let result = analyzer.analyze();
        Ok(PyAnalysisResult { inner: result })
    }
    
    /// Analyze with separate navigation file (new API)
    fn analyze_with_nav(&self, obs_path: &str, nav_path: &str) -> PyResult<PyAnalysisResult> {
        // Read observation file
        let obs_content = std::fs::read(obs_path)
            .map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))?;
        
        let filename = std::path::Path::new(obs_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown.obs");
        
        let obs_data = parse_rinex_obs(&obs_content, filename)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e))?;
        
        // Read navigation file
        let nav_content = std::fs::read(nav_path)
            .map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))?;
        
        let nav_data = parse_navigation(&nav_content)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e))?;
        
        let systems: Vec<char> = self.config.systems.clone()
            .unwrap_or_else(|| vec!["G".to_string(), "R".to_string(), "E".to_string(), "C".to_string()])
            .iter()
            .filter_map(|s| s.chars().next())
            .collect();
        
        let config = AnalysisConfig {
            min_elevation: self.config.min_elevation,
            min_cn0: self.config.min_cn0,
            max_cn0: self.config.max_cn0,
            time_bin_seconds: self.config.time_bin_seconds as u64,
            systems,
            anomaly_threshold_low: self.config.anomaly_threshold_low,
            anomaly_threshold_high: self.config.anomaly_threshold_high,
            anomaly_threshold_critical: self.config.anomaly_threshold_critical,
        };
        
        let analyzer = CN0Analyzer::new(obs_data, config)
            .with_navigation(nav_data);
        
        let result = analyzer.analyze();
        Ok(PyAnalysisResult { inner: result })
    }
}

// ============ Module Functions ============

/// Read RINEX observation file from bytes
#[cfg(feature = "python")]
#[pyfunction]
fn read_rinex_obs_bytes(data: &[u8], filename: &str) -> PyResult<PyRinexObsData> {
    match parse_rinex_obs(data, filename) {
        Ok(obs) => Ok(PyRinexObsData { inner: obs }),
        Err(e) => Err(pyo3::exceptions::PyValueError::new_err(e)),
    }
}

/// Alias for read_rinex_obs_bytes - parse RINEX from bytes
#[cfg(feature = "python")]
#[pyfunction]
fn parse_rinex(data: &[u8], filename: &str) -> PyResult<PyRinexObsData> {
    read_rinex_obs_bytes(data, filename)
}

/// Read RINEX observation file from path
#[cfg(feature = "python")]
#[pyfunction]
fn read_rinex_obs(path: &str) -> PyResult<PyRinexObsData> {
    let content = std::fs::read(path)
        .map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))?;
    
    let filename = std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown.obs");
    
    match parse_rinex_obs(&content, filename) {
        Ok(obs) => Ok(PyRinexObsData { inner: obs }),
        Err(e) => Err(pyo3::exceptions::PyValueError::new_err(e)),
    }
}

/// Read navigation file from bytes
#[cfg(feature = "python")]
#[pyfunction]
fn read_navigation_bytes(data: &[u8]) -> PyResult<PyNavigationData> {
    match parse_navigation(data) {
        Ok(nav) => Ok(PyNavigationData { inner: nav }),
        Err(e) => Err(pyo3::exceptions::PyValueError::new_err(e)),
    }
}

/// Read navigation file from path
#[cfg(feature = "python")]
#[pyfunction]
fn read_navigation(path: &str) -> PyResult<PyNavigationData> {
    let content = std::fs::read(path)
        .map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))?;
    
    match parse_navigation(&content) {
        Ok(nav) => Ok(PyNavigationData { inner: nav }),
        Err(e) => Err(pyo3::exceptions::PyValueError::new_err(e)),
    }
}

/// Quick analysis function
#[cfg(feature = "python")]
#[pyfunction]
#[pyo3(signature = (obs_path, nav_path=None, min_elevation=5.0, systems=None))]
fn analyze_cn0(
    obs_path: &str,
    nav_path: Option<&str>,
    min_elevation: f64,
    systems: Option<Vec<String>>,
) -> PyResult<PyAnalysisResult> {
    // Read observation file
    let obs_content = std::fs::read(obs_path)
        .map_err(|e| pyo3::exceptions::PyIOError::new_err(format!("Cannot read obs file: {}", e)))?;
    
    let filename = std::path::Path::new(obs_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown.obs");
    
    let obs_data = parse_rinex_obs(&obs_content, filename)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e))?;
    
    // Read navigation file if provided
    let nav_data = if let Some(nav_p) = nav_path {
        let nav_content = std::fs::read(nav_p)
            .map_err(|e| pyo3::exceptions::PyIOError::new_err(format!("Cannot read nav file: {}", e)))?;
        
        Some(parse_navigation(&nav_content)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e))?)
    } else {
        None
    };
    
    // Configure and run analysis
    let systems_chars = systems
        .map(|s| s.iter().filter_map(|c| c.chars().next()).collect())
        .unwrap_or_else(|| vec!['G', 'R', 'E', 'C']);
    
    let config = AnalysisConfig {
        min_elevation,
        min_cn0: 0.0,
        max_cn0: 60.0,
        time_bin_seconds: 60,
        systems: systems_chars,
        anomaly_threshold_low: 3.0,
        anomaly_threshold_high: 6.0,
        anomaly_threshold_critical: 10.0,
    };
    
    let mut analyzer = CN0Analyzer::new(obs_data, config);
    
    if let Some(nav) = nav_data {
        analyzer = analyzer.with_navigation(nav);
    }
    
    let result = analyzer.analyze();
    
    Ok(PyAnalysisResult { inner: result })
}

/// Get library version
#[cfg(feature = "python")]
#[pyfunction]
fn version() -> &'static str {
    VERSION
}

/// Download GNSS TLE from CelesTrak
#[cfg(feature = "python")]
#[pyfunction]
fn download_tle() -> PyResult<String> {
    crate::tle::download_gnss_tle()
        .map_err(|e| pyo3::exceptions::PyIOError::new_err(e))
}

/// Parse TLE content and return satellite count
#[cfg(feature = "python")]
#[pyfunction]
fn parse_tle_content(content: &str) -> usize {
    let tles = crate::tle::parse_tle(content);
    tles.len()
}

/// Python module
#[cfg(feature = "python")]
#[pymodule]
fn geoveil_cn0(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("VERSION", VERSION)?;
    m.add("__version__", VERSION)?;
    
    // Classes
    m.add_class::<PyRinexObsData>()?;
    m.add_class::<PyNavigationData>()?;
    m.add_class::<PyAnalysisConfig>()?;
    m.add_class::<PyCN0Analyzer>()?;
    m.add_class::<PyAnalysisResult>()?;
    m.add_class::<PyQualityScore>()?;
    
    // Functions
    m.add_function(wrap_pyfunction!(read_rinex_obs, m)?)?;
    m.add_function(wrap_pyfunction!(read_rinex_obs_bytes, m)?)?;
    m.add_function(wrap_pyfunction!(parse_rinex, m)?)?;
    m.add_function(wrap_pyfunction!(read_navigation, m)?)?;
    m.add_function(wrap_pyfunction!(read_navigation_bytes, m)?)?;
    m.add_function(wrap_pyfunction!(analyze_cn0, m)?)?;
    m.add_function(wrap_pyfunction!(version, m)?)?;
    m.add_function(wrap_pyfunction!(download_tle, m)?)?;
    m.add_function(wrap_pyfunction!(parse_tle_content, m)?)?;
    
    Ok(())
}
