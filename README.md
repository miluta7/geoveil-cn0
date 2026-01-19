# geoveil-cn0

[![PyPI version](https://badge.fury.io/py/geoveil-cn0.svg)](https://pypi.org/project/geoveil-cn0/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Python 3.8+](https://img.shields.io/badge/python-3.8+-blue.svg)](https://www.python.org/downloads/)
[![GitHub](https://img.shields.io/github/stars/miluta7/geoveil-cn0?style=social)](https://github.com/miluta7/geoveil-cn0)

**High-performance GNSS CN0 (Carrier-to-Noise) Signal Quality Analysis Library**

A Rust-powered Python library for analyzing GNSS signal quality from RINEX observation files. Detect interference, jamming, spoofing, and multipath effects with research-grade algorithms.

## Features

- 🛰️ **Multi-GNSS Support**: GPS, GLONASS, Galileo, BeiDou, QZSS, NavIC
- 📊 **CN0 Analysis**: Signal strength statistics, timeseries, and quality scoring
- 🚨 **Threat Detection**: Jamming, spoofing, and interference indicators
- 📈 **Anomaly Detection**: Configurable sensitivity for signal anomalies
- 🗺️ **Skyplot Data**: Satellite positions with azimuth/elevation (requires NAV file)
- ⚡ **High Performance**: Rust core with Python bindings via PyO3
- 📁 **RINEX Support**: Parse RINEX 2.x, 3.x, and 4.x observation files
- 🧭 **Navigation Files**: BRDC broadcast ephemeris and SP3 precise orbits

## Installation

```bash
pip install geoveil-cn0
```

## Quick Start

```python
import geoveil_cn0 as gcn0

# Create analysis configuration
config = gcn0.AnalysisConfig(
    min_elevation=5.0,           # Elevation mask (degrees)
    time_bin_seconds=60,         # Time binning for statistics
    detect_anomalies=True,       # Enable anomaly detection
    anomaly_sensitivity=0.3,     # Sensitivity (0-1, lower = fewer false positives)
    interference_threshold_db=8.0,  # CN0 drop threshold for interference
)

# Create analyzer
analyzer = gcn0.CN0Analyzer(config)

# Analyze observation file (with optional navigation for skyplots)
result = analyzer.analyze_with_nav("observation.rnx", "navigation.rnx")

# Access results
print(f"Quality Score: {result.quality_score.overall}/100 ({result.quality_score.rating})")
print(f"Average CN0: {result.avg_cn0:.1f} dB-Hz")
print(f"Jamming Detected: {result.jamming_detected}")
print(f"Spoofing Detected: {result.spoofing_detected}")
print(f"Anomalies: {result.anomaly_count}")

# Get detailed data
for constellation in result.constellations:
    stats = result.get_constellation_summary(constellation)
    print(f"{constellation}: {stats['satellites_observed']} sats, CN0={stats['cn0_mean']:.1f} dB-Hz")

# Export to JSON
json_data = result.to_json()
```

## Quality Score Components

The quality score (0-100) is computed from multiple factors:

| Component | Weight | Description |
|-----------|--------|-------------|
| CN0 Quality | 35% | Signal strength relative to thresholds |
| Availability | 20% | Satellite availability vs expected |
| Continuity | 20% | Data continuity (gaps, cycle slips) |
| Stability | 15% | CN0 variance over time |
| Diversity | 10% | Multi-constellation coverage |

## Threat Detection Thresholds

Based on research from ITU-R M.1902-1, Stanford GPS Lab, and GPS Solutions journal:

- **Jamming**: Rapid CN0 drops >6 dB in <3 seconds
- **Spoofing**: Abnormally uniform CN0 (std <2 dB) or elevated average
- **Interference**: CN0 degradation >4 dB from baseline (ITU I/N=-6dB criterion)

## API Reference

### AnalysisConfig

```python
config = gcn0.AnalysisConfig(
    min_elevation=5.0,              # Elevation cutoff in degrees
    time_bin_seconds=60,            # Time bin for statistics
    systems=['G', 'R', 'E', 'C'],   # GNSS systems to analyze
    detect_anomalies=True,          # Enable anomaly detection
    anomaly_sensitivity=0.3,        # 0-1, lower = stricter
    interference_threshold_db=8.0,  # dB drop for interference flag
    verbose=False,                  # Print debug info
)
```

### CN0Analyzer

```python
analyzer = gcn0.CN0Analyzer(config)

# Analyze without navigation (no skyplots)
result = analyzer.analyze_file("observation.rnx")

# Analyze with navigation (enables skyplots and elevation filtering)
result = analyzer.analyze_with_nav("observation.rnx", "navigation.rnx")
```

### AnalysisResult

| Property | Type | Description |
|----------|------|-------------|
| `quality_score` | QualityScore | Overall quality metrics |
| `avg_cn0` | float | Mean CN0 in dB-Hz |
| `cn0_std_dev` | float | CN0 standard deviation |
| `min_cn0`, `max_cn0` | float | CN0 range |
| `jamming_detected` | bool | Jamming indicator |
| `spoofing_detected` | bool | Spoofing indicator |
| `interference_detected` | bool | Interference indicator |
| `anomaly_count` | int | Number of anomalies |
| `constellations` | list | Available constellations |
| `duration_hours` | float | Observation duration |
| `epoch_count` | int | Number of epochs |

### Methods

```python
# Get constellation-specific statistics
stats = result.get_constellation_summary("GPS")  # Returns dict

# Get anomaly list
anomalies = result.get_anomalies()  # Returns list of dicts

# Get timeseries data for plotting
ts_data = result.get_timeseries_data()  # Returns dict with timestamps, cn0_mean, etc.

# Get skyplot data (requires navigation file)
skyplot = result.get_skyplot_data()  # Returns list of satellite traces

# Export to JSON
json_str = result.to_json()
```

## Jupyter Notebook Widget

A ready-to-use interactive widget is included:

```python
# In Jupyter notebook
exec(open("geoveil_cn0_gui.py").read())
```

Features:
- File upload or path input
- Auto-download BRDC navigation files
- Interactive Plotly charts
- Quality radar, skyplots, heatmaps, timeseries
- HTML report export

## Data Sources

The library supports navigation data from:

- **BRDC**: IGS combined broadcast ephemeris
- **SP3**: Precise orbits from ESA, GFZ, CODE, WHU
- **TLE**: CelesTrak GNSS TLE (fallback)

## Requirements

- Python 3.8+
- No runtime dependencies (Rust binary)

Optional for notebooks:
- `plotly` - Interactive charts
- `pandas` - Data manipulation
- `ipywidgets` - Jupyter widgets

## License

MIT License - see [LICENSE](LICENSE) for details.

## Author

**Miluta Dulea-Flueras**

## Contributing

Contributions welcome! Please open an issue or pull request on [GitHub](https://github.com/miluta7/geoveil-cn0).

## Citation

If you use this library in research, please cite:

```bibtex
@software{geoveil_cn0,
  author = {Dulea-Flueras, Miluta},
  title = {geoveil-cn0: GNSS CN0 Signal Quality Analysis Library},
  year = {2026},
  url = {https://github.com/miluta7/geoveil-cn0}
}
```
