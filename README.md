# geoveil-cn0

[![PyPI version](https://badge.fury.io/py/geoveil-cn0.svg)](https://pypi.org/project/geoveil-cn0/)
[![PyPI downloads](https://img.shields.io/pypi/dm/geoveil-cn0.svg)](https://pypi.org/project/geoveil-cn0/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Python 3.9+](https://img.shields.io/badge/python-3.9+-blue.svg)](https://www.python.org/downloads/)
[![Rust](https://img.shields.io/badge/powered%20by-Rust-orange.svg)](https://www.rust-lang.org/)
[![GitHub Stars](https://img.shields.io/github/stars/miluta7/geoveil-cn0?style=social)](https://github.com/miluta7/geoveil-cn0)

**High-performance GNSS signal quality analysis library — Rust core, Python API**

Analyze RINEX observation files to compute signal quality scores, detect threats (jamming, spoofing, interference), generate per-constellation statistics, and produce skyplot data. Used in production geodetic monitoring, precision agriculture, and GNSS security research.

---

## Installation

```bash
pip install geoveil-cn0
```

No Rust toolchain required — pre-built wheels for Linux (x86\_64 + ARM/piwheels), Windows, and macOS.

---

## Quick Start

```python
import geoveil_cn0 as gcn0

print(gcn0.VERSION)  # e.g. "0.3.7"

# Configure analysis
config = gcn0.AnalysisConfig(
    min_elevation=5.0,            # Elevation mask in degrees
    time_bin=60,                  # Time-binning interval for statistics (seconds)
    systems=["G", "R", "E", "C"], # GPS, GLONASS, Galileo, BeiDou
    detect_anomalies=True,
    anomaly_sensitivity=0.3,      # 0.1 = very sensitive, 1.0 = major events only
    interference_threshold_db=8.0,
)

analyzer = gcn0.CN0Analyzer(config)

# Analyze with BRDC navigation file (enables elevation filtering + skyplots)
result = analyzer.analyze_with_nav("observation.rnx", "navigation.rnx")

# — or without navigation (CN0 analysis only) —
result = analyzer.analyze_file("observation.rnx")

# Core metrics
print(f"Quality: {result.quality_score.overall:.1f}/100 ({result.quality_score.rating})")
print(f"Mean CN0: {result.avg_cn0:.1f} dB-Hz  std: {result.cn0_std_dev:.1f}")
print(f"Jamming: {result.jamming_detected}  Spoofing: {result.spoofing_detected}")
print(f"Anomalies: {result.anomaly_count}")

# Per-constellation breakdown
for sys in result.get_systems():
    s = result.get_constellation_summary(sys)
    print(f"  {sys}: {s['satellites_observed']} sats, CN0={s['cn0_mean']:.1f} dB-Hz, avail={s['availability_ratio']}")

# Export full result
json_str = result.to_json()
```

---

## Supported File Formats

| Type | Extensions | Versions |
|------|-----------|---------|
| Observation | `.obs`, `.rnx`, `.crx`, `.YYo` (e.g. `.26o`) | RINEX 2.x / 3.x / 4.x |
| Navigation | `.nav`, `.rnx`, `.YYn`, `.YYg`, `.YYp` | BRDC mixed / GPS / GLONASS |
| Precise orbits | `.sp3`, `.SP3` | SP3-c, SP3-d |
| Compressed | `.gz`, `.Z`, `.crx` | Gzip, Unix-compress, Hatanaka |

---

## Quality Score

Composite 0–100 score with letter rating (A–F), weighted from five components:

| Component | Weight | Basis |
|-----------|--------|-------|
| CN0 Quality | 35% | Signal strength vs. constellation thresholds |
| Availability | 20% | Observed vs. expected satellite count |
| Continuity | 20% | Data-gap and cycle-slip rate |
| Stability | 15% | CN0 variance over observation window |
| Diversity | 10% | Multi-constellation coverage |

---

## Threat Detection

Algorithms derived from ITU-R M.1902-1, Stanford GPS Lab, and GPS Solutions journal:

| Threat | Detection Method | Default Threshold |
|--------|-----------------|-----------|
| **Jamming** | Rapid CN0 drop rate | >6 dB in <3 seconds |
| **Spoofing** | CN0 uniformity anomaly | std <2 dB with elevated mean |
| **Interference** | Sustained CN0 degradation | >4 dB from baseline (ITU I/N=−6 dB) |

All thresholds are configurable via `AnalysisConfig`.

---

## API Reference

### `AnalysisConfig`

```python
config = gcn0.AnalysisConfig(
    min_elevation=5.0,              # Elevation cutoff (degrees)
    time_bin=60,                    # Statistics time bin (seconds)
    systems=["G", "R", "E", "C"],   # G=GPS R=GLONASS E=Galileo C=BeiDou J=QZSS I=NavIC
    detect_anomalies=True,
    anomaly_sensitivity=0.3,        # 0.1–1.0
    interference_threshold_db=8.0,  # CN0 drop threshold (dB)
    verbose=False,
)
```

### `CN0Analyzer`

| Method | Parameters | Returns |
|--------|-----------|---------|
| `analyze_file(obs_path)` | RINEX obs path | `AnalysisResult` |
| `analyze_with_nav(obs_path, nav_path)` | obs + nav/SP3 path | `AnalysisResult` |

### `AnalysisResult` — Properties

| Property | Type | Unit | Description |
|----------|------|------|-------------|
| `quality_score` | `QualityScore` | — | Composite quality object |
| `avg_cn0` / `mean_cn0` | float | dB-Hz | Mean carrier-to-noise ratio |
| `cn0_std_dev` | float | dB-Hz | CN0 standard deviation |
| `min_cn0`, `max_cn0` | float | dB-Hz | CN0 range |
| `jamming_detected` | bool | — | Jamming indicator |
| `spoofing_detected` | bool | — | Spoofing indicator |
| `interference_detected` | bool | — | Interference indicator |
| `anomaly_count` | int | — | Total anomaly events |
| `duration_hours` | float | h | Observation window length |
| `epoch_count` | int | — | Number of epochs parsed |
| `constellations` | list[str] | — | Constellation names present |
| `rinex_version` | str | — | File format version |
| `station_name` | str | — | Marker name from RINEX header |

### `AnalysisResult` — Methods

```python
result.get_systems()                      # -> List[str]   constellation codes
result.get_constellation_summary("GPS")   # -> Dict        per-constellation stats
result.get_anomalies()                    # -> List[Dict]  anomaly event list
result.get_timeseries_data()              # -> Dict        time-binned CN0 series
result.get_timestamps()                   # -> List[str]   ISO timestamp strings
result.get_mean_cn0_series()              # -> List[float]
result.get_satellite_count_series()       # -> List[int]
result.get_skyplot_data()                 # -> List[Dict]  satellite az/el tracks (needs nav)
result.to_json()                          # -> str         full JSON export
```

### `QualityScore`

```python
qs = result.quality_score
qs.overall        # float  0–100
qs.rating         # str    "A (Excellent)" … "F (Very Poor)"
qs.cn0_quality    # float  component score
qs.availability   # float  component score
qs.continuity     # float  component score
qs.stability      # float  component score
qs.diversity      # float  component score
```

---

## Jupyter Notebook Widget

An interactive analysis widget is included (`notebooks/geoveil-cn0-widget.ipynb`):

```bash
pip install geoveil-cn0 plotly pandas ipywidgets matplotlib
jupyter notebook notebooks/geoveil-cn0-widget.ipynb
```

Features:
- RINEX file upload or path input
- BRDC navigation auto-download from IGS/MGEX
- Interactive Plotly charts: CN0 timeseries, skyplot, signal heatmap, quality radar
- Per-constellation signal statistics
- Anomaly event list
- One-click HTML report export

---

## Batch Processing

For large-scale production use, [geoveil-cn0-batch](https://github.com/miluta7/geoveil-cn0) wraps this library in a scalable microservices stack:

- **FastAPI** REST + WebSocket API
- **Celery** + Redis distributed task queue
- **MongoDB** result storage
- **MinIO** RINEX and graph data storage
- **React** web dashboard with live progress, skyplots, and export

Supports hundreds of RINEX files per session with per-session analysis settings, automatic ephemeris download, and multi-worker horizontal scaling.

---

## Requirements

- Python 3.9+
- No runtime dependencies (pure Rust binary)

Optional (notebooks):
- `plotly`, `pandas`, `ipywidgets`, `matplotlib`

---

## License

MIT — see [LICENSE](LICENSE).

## Author

**Miluta Dulea-Flueras** — [miluta.flueras@cartografie.ro](mailto:miluta.flueras@cartografie.ro)

## Contributing

Issues and pull requests welcome at [github.com/miluta7/geoveil-cn0](https://github.com/miluta7/geoveil-cn0).

## Citation

```bibtex
@software{geoveil_cn0,
  author    = {Dulea-Flueras, Miluta},
  title     = {geoveil-cn0: High-Performance GNSS CN0 Signal Quality Analysis Library},
  year      = {2026},
  version   = {0.3.7},
  url       = {https://github.com/miluta7/geoveil-cn0},
  license   = {MIT}
}
```
