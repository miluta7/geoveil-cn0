# geoveil-cn0 — Product Pitch

## What It Is

**geoveil-cn0** is an open-source Python library with a Rust core for analyzing GNSS signal quality from RINEX observation files. It turns raw receiver data into actionable intelligence: quality scores, threat flags, per-constellation statistics, and skyplot visualizations — in milliseconds per file.

```bash
pip install geoveil-cn0
```

---

## The Problem

GNSS receivers collect terabytes of RINEX data annually across geodetic networks, reference stations, precision agriculture fleets, and autonomous vehicle test sites. Manually reviewing signal quality is impractical at scale. Existing tools (TEQC, RTKLIB, Anubis) are designed for engineers, not automation — they produce reports, not programmatic data.

Meanwhile, GNSS jamming and spoofing incidents are increasing globally. The EU GNSS Agency (EUSPA) reports hundreds of interference events per year affecting aviation, maritime, and critical infrastructure. Most operators detect these events hours or days after the fact.

---

## The Solution

geoveil-cn0 provides:

### 1. Real-Time Quality Scoring
A composite 0–100 score computed from five components (CN0 quality, satellite availability, data continuity, signal stability, constellation diversity). Results in <1 second per file on commodity hardware.

### 2. Threat Detection
Three independent detectors running on every file:
- **Jamming**: rapid CN0 drop rate >6 dB in <3s (Stanford GPS Lab algorithm)
- **Spoofing**: CN0 uniformity anomaly — abnormally low variance with elevated mean
- **Interference**: sustained degradation >4 dB from baseline (ITU-R M.1902-1 criterion)

All thresholds are configurable per deployment context.

### 3. Full GNSS Constellation Coverage
GPS · GLONASS · Galileo · BeiDou · QZSS · NavIC — six constellations in a single analysis pass.

### 4. Skyplot Generation
Satellite azimuth/elevation tracks computed from BRDC or SP3 ephemeris, ready for visualization. Supports Hatanaka-compressed RINEX.

### 5. Zero-Dependency Runtime
The Rust binary ships as a compiled Python extension (`.so` / `.pyd`). No native library dependencies, no C compiler, no Rust toolchain at runtime. `pip install` and go.

### 6. Production-Ready Integration
```python
result = analyzer.analyze_with_nav("station.rnx", "brdc.nav")
if result.jamming_detected or result.spoofing_detected:
    alert(station_id, result.quality_score.overall, result.get_anomalies())
```

---

## Performance

| File size | Epochs | Constellations | Analysis time |
|-----------|--------|----------------|--------------|
| 2 MB | ~2,880 (30s, 24h) | 4 | ~0.3s |
| 25 MB | ~86,400 (1s, 24h) | 4 | ~3s |
| 120 MB | ~432,000 (1s, 5-day) | 4 | ~12s |

Rust core eliminates Python GIL contention. ThreadPoolExecutor parallelism scales linearly with CPU cores in batch workloads.

---

## Use Cases

### Geodetic Network Monitoring
Continuously score RINEX files from CORS/ROMPOS/IGS stations as they arrive. Flag degraded stations before they corrupt reference data. Track quality trends over time per marker.

### GNSS Security & Threat Intelligence
Detect jamming events correlated with geopolitical events, military exercises, or criminal activity. Export anomaly timelines for incident reporting and forensic analysis.

### Precision Agriculture
Validate signal quality for RTK correction services before field operations. Alert when base station CN0 drops below operational thresholds.

### Autonomous Vehicles & Drones
Integrate into post-mission analysis pipelines to classify GNSS outages by cause (jamming vs. multipath vs. satellite geometry) for safety certification.

### Research & Academia
Programmable interface for scripting quality assessments across large RINEX archives. Full JSON export for downstream ML/statistical analysis.

---

## Deployment Options

### Standalone (pip)
```bash
pip install geoveil-cn0
python analyze.py station_2026_132.rnx
```

### Jupyter Widget
Interactive analysis dashboard with Plotly charts, skyplot, heatmap, and one-click HTML report. No code required.

### Batch Processing Stack (geoveil-cn0-batch)
Full production system built on this library:
- FastAPI REST + WebSocket API
- Celery + Redis distributed queue
- MongoDB + MinIO storage
- React web dashboard
- Docker Compose single-command deployment
- Scales to hundreds of files per session across multiple workers

---

## Technical Highlights

- **Language**: Rust (core) + Python (API) via PyO3
- **Builds**: manylinux2014 x86\_64, ARM (piwheels), Windows, macOS
- **Python**: 3.9 – 3.12
- **RINEX**: 2.x, 3.x, 4.x + Hatanaka compression
- **Ephemeris**: BRDC (IGS/MGEX), SP3 (precise orbits)
- **License**: MIT
- **CI/CD**: GitHub Actions → PyPI (automatic on version tag)

---

## Adoption

- Active production deployment at Romanian national geodetic network (ROMPOS)
- Processing 30-day rolling RINEX archive from 60+ reference stations
- Integrated into GeoVeil Batch — a multi-tenant web platform for RINEX analysis

---

## Roadmap

- [ ] Multipath residual integration (geoveil-mp companion library)
- [ ] Real-time streaming RINEX input (NTRIP/RTCM → live quality feed)
- [ ] GNSS threat severity classification (severity levels 1–5)
- [ ] REST API microservice packaging
- [ ] Time-series database export (InfluxDB, TimescaleDB)

---

## Links

- **PyPI**: https://pypi.org/project/geoveil-cn0/
- **GitHub**: https://github.com/miluta7/geoveil-cn0
- **Author**: Miluta Dulea-Flueras — miluta.flueras@cartografie.ro
