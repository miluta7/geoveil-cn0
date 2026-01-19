//! Core types for CN0 analysis
//!
//! IMPORTANT: All HashMap keys MUST be String for JSON compatibility

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// GNSS System identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GnssSystem {
    GPS,
    GLONASS,
    Galileo,
    BeiDou,
    QZSS,
    SBAS,
}

impl GnssSystem {
    pub fn from_char(c: char) -> Option<Self> {
        match c {
            'G' => Some(Self::GPS),
            'R' => Some(Self::GLONASS),
            'E' => Some(Self::Galileo),
            'C' => Some(Self::BeiDou),
            'J' => Some(Self::QZSS),
            'S' => Some(Self::SBAS),
            _ => None,
        }
    }
    
    pub fn to_char(&self) -> char {
        match self {
            Self::GPS => 'G',
            Self::GLONASS => 'R',
            Self::Galileo => 'E',
            Self::BeiDou => 'C',
            Self::QZSS => 'J',
            Self::SBAS => 'S',
        }
    }
    
    pub fn name(&self) -> &'static str {
        match self {
            Self::GPS => "GPS",
            Self::GLONASS => "GLONASS",
            Self::Galileo => "Galileo",
            Self::BeiDou => "BeiDou",
            Self::QZSS => "QZSS",
            Self::SBAS => "SBAS",
        }
    }
}

/// Epoch (time representation)
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Epoch {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: f64,
}

impl Epoch {
    pub fn new(year: i32, month: u32, day: u32, hour: u32, minute: u32, second: f64) -> Self {
        Self { year, month, day, hour, minute, second }
    }
    
    pub fn seconds_of_day(&self) -> f64 {
        self.hour as f64 * 3600.0 + self.minute as f64 * 60.0 + self.second
    }
    
    pub fn to_gps_time(&self) -> (i32, f64) {
        let jd = self.julian_date();
        let gps_epoch_jd = 2444244.5;
        let days_since = jd - gps_epoch_jd;
        let week = (days_since / 7.0).floor() as i32;
        let tow = (days_since - week as f64 * 7.0) * 86400.0;
        (week, tow)
    }
    
    pub fn julian_date(&self) -> f64 {
        let y = self.year as f64;
        let m = self.month as f64;
        let d = self.day as f64 + self.seconds_of_day() / 86400.0;
        
        let a = ((14.0 - m) / 12.0).floor();
        let y_adj = y + 4800.0 - a;
        let m_adj = m + 12.0 * a - 3.0;
        
        d + ((153.0 * m_adj + 2.0) / 5.0).floor()
            + 365.0 * y_adj
            + (y_adj / 4.0).floor()
            - (y_adj / 100.0).floor()
            + (y_adj / 400.0).floor()
            - 32045.0
    }
    
    pub fn day_of_year(&self) -> u32 {
        let days_in_month = [0, 31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
        let is_leap = (self.year % 4 == 0 && self.year % 100 != 0) || (self.year % 400 == 0);
        
        let mut doy: u32 = 0;
        for i in 1..self.month {
            doy += days_in_month[i as usize];
            if i == 2 && is_leap {
                doy += 1;
            }
        }
        doy + self.day
    }
    
    pub fn diff_seconds(&self, other: &Epoch) -> f64 {
        (self.julian_date() - other.julian_date()) * 86400.0
    }
    
    pub fn to_iso_string(&self) -> String {
        format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:06.3}Z",
            self.year, self.month, self.day,
            self.hour, self.minute, self.second)
    }
    
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.replace('T', " ").replace('Z', "");
        let parts: Vec<&str> = s.split(|c| c == '-' || c == ' ' || c == ':')
            .filter(|p| !p.is_empty())
            .collect();
        
        if parts.len() >= 6 {
            Some(Self {
                year: parts[0].parse().ok()?,
                month: parts[1].parse().ok()?,
                day: parts[2].parse().ok()?,
                hour: parts[3].parse().ok()?,
                minute: parts[4].parse().ok()?,
                second: parts[5].parse().ok()?,
            })
        } else {
            None
        }
    }
}

impl std::fmt::Display for Epoch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:04}-{:02}-{:02}T{:02}:{:02}:{:06.3}Z",
            self.year, self.month, self.day,
            self.hour, self.minute, self.second)
    }
}

/// ECEF coordinates (meters)
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct Ecef {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Ecef {
    pub fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }
    
    pub fn to_geodetic(&self) -> (f64, f64, f64) {
        const A: f64 = 6378137.0;
        const F: f64 = 1.0 / 298.257223563;
        const E2: f64 = F * (2.0 - F);
        
        let lon = self.y.atan2(self.x);
        let p = (self.x * self.x + self.y * self.y).sqrt();
        
        let mut lat = self.z.atan2(p * (1.0 - E2));
        for _ in 0..10 {
            let sin_lat = lat.sin();
            let n = A / (1.0 - E2 * sin_lat * sin_lat).sqrt();
            lat = (self.z + E2 * n * sin_lat).atan2(p);
        }
        
        let sin_lat = lat.sin();
        let n = A / (1.0 - E2 * sin_lat * sin_lat).sqrt();
        let height = if lat.cos().abs() > 1e-10 {
            p / lat.cos() - n
        } else {
            self.z.abs() - n * (1.0 - E2)
        };
        
        (lat.to_degrees(), lon.to_degrees(), height)
    }
    
    pub fn magnitude(&self) -> f64 {
        (self.x * self.x + self.y * self.y + self.z * self.z).sqrt()
    }
}

/// Single CN0 observation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CN0Observation {
    pub timestamp: String,
    pub satellite: String,
    pub system: String,
    pub prn: u32,
    pub signal: String,
    pub cn0: f64,
    pub elevation: f64,
    pub azimuth: f64,
}

/// Anomaly severity level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnomalySeverity {
    Low,
    High,
    Critical,
}

impl AnomalySeverity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }
}

/// Detected anomaly event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalyEvent {
    pub timestamp: String,
    pub severity: String,
    pub event_type: String,
    pub description: String,
    pub cn0_drop: f64,
    pub affected_satellites: Vec<String>,
    pub affected_systems: Vec<String>,
}

/// Per-satellite statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SatelliteStats {
    pub satellite: String,
    pub system: String,
    pub observation_count: usize,
    pub mean_cn0: f64,
    pub min_cn0: f64,
    pub max_cn0: f64,
    pub std_cn0: f64,
    pub mean_elevation: f64,
}

/// Per-constellation statistics  
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstellationStats {
    pub system: String,
    pub satellite_count: usize,
    pub observation_count: usize,
    pub mean_cn0: f64,
    pub min_cn0: f64,
    pub max_cn0: f64,
    pub std_cn0: f64,
    /// Expected number of satellites in constellation
    pub satellites_expected: usize,
    /// Ratio of observed to expected satellites (0.0 - 1.0)
    pub availability_ratio: f64,
    /// Number of detected cycle slips
    pub cycle_slips: usize,
    /// Number of data gaps detected
    pub data_gaps: usize,
}

/// Timeseries data point
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeseriesPoint {
    pub timestamp: String,
    pub cn0: f64,
    pub elevation: f64,
    pub azimuth: f64,
}

/// Satellite timeseries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SatelliteTimeseries {
    pub satellite: String,
    pub system: String,
    pub signal: String,
    pub cn0_series: Vec<TimeseriesPoint>,
}

/// Skyplot data point
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkyplotPoint {
    pub azimuth: f64,
    pub elevation: f64,
    pub cn0: f64,
    pub satellite: String,
}

/// Skyplot trace for one satellite
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkyplotTrace {
    pub satellite: String,
    pub system: String,
    pub points: Vec<SkyplotPoint>,
}

/// Heatmap data point
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeatmapPoint {
    pub azimuth_bin: f64,
    pub elevation_bin: f64,
    pub mean_cn0: f64,
    pub count: usize,
}

/// Quality score
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityScore {
    pub overall: f64,
    pub cn0_score: f64,
    pub coverage_score: f64,
    pub consistency_score: f64,
    pub grade: String,
    pub interpretation: String,
}

/// File information
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileInfo {
    pub filename: String,
    pub marker_name: String,
    pub receiver_type: String,
    pub antenna_type: String,
    pub start_time: String,
    pub end_time: String,
    pub duration_hours: f64,
    pub interval: f64,
    pub position_lat: f64,
    pub position_lon: f64,
    pub position_height: f64,
}

/// Analysis summary
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AnalysisSummary {
    pub total_observations: usize,
    pub total_satellites: usize,
    pub total_epochs: usize,
    pub systems_observed: Vec<String>,
    pub mean_cn0: f64,
    pub min_cn0: f64,
    pub max_cn0: f64,
    pub anomaly_count: usize,
    pub critical_count: usize,
    pub high_count: usize,
    pub low_count: usize,
}

/// Timeseries data container
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TimeseriesData {
    pub timestamps: Vec<String>,
    pub mean_cn0: Vec<f64>,
    pub satellite_count: Vec<usize>,
    /// KEY IS STRING (satellite ID like "G01")
    pub satellite_timeseries: HashMap<String, SatelliteTimeseries>,
    /// KEY IS STRING (system char like "G")
    pub constellation_timeseries: HashMap<String, Vec<TimeseriesPoint>>,
}

/// Skyplot data container
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SkyplotData {
    pub traces: Vec<SkyplotTrace>,
    pub heatmap_points: Vec<HeatmapPoint>,
    pub coverage_percent: f64,
}

/// Heatmap data container
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HeatmapData {
    pub azimuth_bins: Vec<f64>,
    pub elevation_bins: Vec<f64>,
    pub cn0_matrix: Vec<Vec<f64>>,
    pub count_matrix: Vec<Vec<usize>>,
}

/// Per-system visibility statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SystemVisibilityStats {
    pub system: String,
    pub predicted_satellites: usize,
    pub observed_satellites: usize,
    pub confirmation_rate: f64,
    pub missing_satellites: Vec<String>,
    pub unexpected_satellites: Vec<String>,
}

/// Visibility anomaly info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisibilityAnomalyInfo {
    pub start_time: String,
    pub end_time: String,
    pub duration_seconds: f64,
    pub anomaly_type: String,
    pub severity: String,
    pub affected_satellites: Vec<String>,
    pub affected_systems: Vec<String>,
    pub description: String,
}

/// Full analysis result - ALL KEYS ARE STRINGS for JSON compatibility
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisResult {
    pub file_info: FileInfo,
    pub quality_score: QualityScore,
    pub summary: AnalysisSummary,
    /// KEY IS STRING (system name like "GPS")
    pub constellation_stats: HashMap<String, ConstellationStats>,
    /// KEY IS STRING (satellite ID like "G01")
    pub satellite_stats: HashMap<String, SatelliteStats>,
    pub timeseries: TimeseriesData,
    pub skyplot: SkyplotData,
    pub heatmap: HeatmapData,
    pub anomalies: Vec<AnomalyEvent>,
    /// Visibility assessment (predicted vs observed satellites)
    pub visibility: Option<VisibilityData>,
}

/// Visibility assessment data
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VisibilityData {
    pub has_prediction: bool,
    pub prediction_source: String,
    pub confirmation_rate: f64,
    pub mean_predicted: f64,
    pub mean_observed: f64,
    pub mean_missing: f64,
    pub mean_unexpected: f64,
    pub frequently_missing: Vec<String>,
    pub frequently_unexpected: Vec<String>,
    /// Per-system breakdown (key = system name)
    pub by_system: HashMap<String, SystemVisibilityStats>,
    /// Visibility anomalies
    pub anomalies: Vec<VisibilityAnomalyInfo>,
}

impl Default for AnalysisResult {
    fn default() -> Self {
        Self {
            file_info: FileInfo::default(),
            quality_score: QualityScore {
                overall: 0.0,
                cn0_score: 0.0,
                coverage_score: 0.0,
                consistency_score: 0.0,
                grade: "N/A".to_string(),
                interpretation: "No data".to_string(),
            },
            summary: AnalysisSummary::default(),
            constellation_stats: HashMap::new(),
            satellite_stats: HashMap::new(),
            timeseries: TimeseriesData::default(),
            skyplot: SkyplotData::default(),
            heatmap: HeatmapData::default(),
            anomalies: Vec::new(),
            visibility: None,
        }
    }
}

impl AnalysisResult {
    /// Convert to JSON string - ALL KEYS ARE STRINGS
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|e| {
            format!("{{\"error\": \"{}\"}}", e)
        })
    }
    
    /// Convert to pretty JSON string
    pub fn to_json_pretty(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|e| {
            format!("{{\"error\": \"{}\"}}", e)
        })
    }
}

/// Analysis configuration
#[derive(Debug, Clone)]
pub struct AnalysisConfig {
    pub min_elevation: f64,
    pub min_cn0: f64,
    pub max_cn0: f64,
    pub time_bin_seconds: u64,
    pub systems: Vec<char>,
    pub anomaly_threshold_low: f64,
    pub anomaly_threshold_high: f64,
    pub anomaly_threshold_critical: f64,
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        Self {
            min_elevation: 5.0,
            min_cn0: 0.0,
            max_cn0: 60.0,
            time_bin_seconds: 60,
            systems: vec!['G', 'R', 'E', 'C'],
            anomaly_threshold_low: 3.0,
            anomaly_threshold_high: 6.0,
            anomaly_threshold_critical: 10.0,
        }
    }
}

// ============ RINEX Observation Data Types ============

/// Observation value
#[derive(Debug, Clone, Copy, Default)]
pub struct ObsValue {
    pub value: f64,
    pub lli: Option<u8>,
    pub ssi: Option<u8>,
}

/// Satellite observations at one epoch
#[derive(Debug, Clone, Default)]
pub struct SatelliteObs {
    /// KEY IS STRING (observation code like "C1C", "S1C")
    pub values: HashMap<String, ObsValue>,
}

/// Single epoch of RINEX observations
#[derive(Debug, Clone)]
pub struct EpochData {
    pub epoch: Epoch,
    pub flag: u8,
    /// KEY IS STRING (satellite ID like "G01")
    pub observations: HashMap<String, SatelliteObs>,
}

/// Parsed RINEX observation data
#[derive(Debug, Clone, Default)]
pub struct RinexObsData {
    pub version: f64,
    pub marker_name: String,
    pub receiver_type: String,
    pub antenna_type: String,
    pub approx_position: Option<Ecef>,
    pub interval: f64,
    /// KEY IS STRING (system char as string like "G")
    pub obs_types: HashMap<String, Vec<String>>,
    pub epochs: Vec<EpochData>,
    /// KEY IS STRING (PRN as string like "1", "24")
    pub glonass_fcn: HashMap<String, i32>,
}

impl RinexObsData {
    pub fn num_epochs(&self) -> usize {
        self.epochs.len()
    }
    
    pub fn num_satellites(&self) -> usize {
        let mut sat_ids: Vec<String> = Vec::new();
        for epoch in &self.epochs {
            for sat_id in epoch.observations.keys() {
                if !sat_ids.contains(sat_id) {
                    sat_ids.push(sat_id.clone());
                }
            }
        }
        sat_ids.len()
    }
    
    pub fn first_epoch(&self) -> Option<&Epoch> {
        self.epochs.first().map(|e| &e.epoch)
    }
    
    pub fn last_epoch(&self) -> Option<&Epoch> {
        self.epochs.last().map(|e| &e.epoch)
    }
    
    /// Returns satellites by system - KEY IS STRING
    pub fn satellites_by_system(&self) -> HashMap<String, Vec<String>> {
        let mut result: HashMap<String, Vec<String>> = HashMap::new();
        for epoch in &self.epochs {
            for sat_id in epoch.observations.keys() {
                if let Some(sys_char) = sat_id.chars().next() {
                    let sys_str = sys_char.to_string();
                    let list = result.entry(sys_str).or_insert_with(Vec::new);
                    if !list.contains(sat_id) {
                        list.push(sat_id.clone());
                    }
                }
            }
        }
        result
    }
}
