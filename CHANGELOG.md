# Changelog

All notable changes to geoveil-cn0 are documented here.
Format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

---

## [0.3.8] — 2026-06-15

### Fixed
- **Systematic spoofing false positives on clean reference stations** (`types.rs`):
  `to_gps_time()` used the Julian Date formula which returns noon-based JD, but
  compared it against `gps_epoch_jd = 2444244.5` (midnight-based). This caused a
  systematic +43200 s (12 h) error in GPS Time of Week, making all Keplerian
  satellite positions wildly wrong. Result: every BRDC-nav analysis flagged
  ~60% of observed satellites as "unexpected", triggering `spoofing_detected=True`
  on all clean data. Fix: subtract 0.5 from the JD before computing elapsed days
  (`let jd = self.julian_date() - 0.5;`). GLONASS was unaffected (uses
  `diff_seconds()` which cancels the bias). After fix: unexpected ratio drops from
  ~63% to ~8% on ROMPOS reference stations; `confirmation_rate` rises from ~33%
  to ~85%.

### Changed
- **Spoofing detector now requires corroboration** (`python.rs`): primary threshold
  alone (unexpected ratio > 40%, count > 8) no longer fires; detection also requires
  either a sustained anomaly event (>300 s) or an overwhelming ratio (>60%).
  Eliminates residual false positives from short noise bursts while preserving true
  detection sensitivity.

### Added
- `spoofing_unexpected_threshold` and `spoofing_min_unexpected_count` fields on
  `AnalysisConfig` and `AnalysisResult`, allowing per-deployment tuning of
  the spoofing detection sensitivity.

---


## [0.3.7] — 2026-05-18

### Fixed
- **Panic on empty satellite ID** (`cn0.rs:180`): `sat_id[1..]` panicked with
  *"byte index 1 is out of bounds"* on RINEX observation records containing an
  empty satellite identifier string (e.g. malformed RINEX 2 files from certain
  TPS/Topcon receivers). Replaced with `sat_id.get(1..).unwrap_or("")` which is
  bounds-safe and treats such records as PRN 0 (skipped).

### Added
- Source distribution (sdist) now published to PyPI — enables piwheels ARM builds
  and `pip install geoveil-cn0` from source on unsupported platforms.

---

## [0.3.6] — 2026-04-26

### Changed
- manylinux2014 compatibility: switched to `manylinux: auto` in CI to produce
  broader-compatible Linux wheels (previously required glibc ≥ 2.28).

### Fixed
- Build failure on Python 3.12 due to deprecated PyO3 ABI flags.

---

## [0.3.5] — 2026-01-20

### Added
- `get_skyplot_data()` now returns per-satellite azimuth/elevation tracks parsed
  directly from BRDC/SP3 ephemeris; no longer requires SP3 for skyplot generation.
- Hatanaka compressed RINEX (`.crx`, `.YYd`) decompression support.
- `lock_integrity_score` quality sub-component replacing the deprecated
  `lock_loss_score`.

### Fixed
- Spoofing false-positive rate reduced on high-quality geodetic receivers
  (Leica, Trimble) with naturally tight CN0 distributions.

---

## [0.3.4] — 2026-01-19

### Added
- BeiDou (C) constellation full support: B1I, B2I, B3I signals.
- QZSS (J) and NavIC/IRNSS (I) constellation parsing.
- `diversity` quality component measuring multi-constellation coverage.

---

## [0.3.3] — 2026-01-19

### Added
- SP3 precise orbit support for elevation computation (fallback to BRDC).
- `get_timeseries_data()` returns time-binned CN0, satellite count, and timestamp arrays.

### Changed
- Quality score rebalanced: CN0 weight 35%, Availability 20%, Continuity 20%,
  Stability 15%, Diversity 10%.

---

## [0.3.2] — 2026-01-19

### Added
- Initial multi-GNSS support: GPS (G), GLONASS (R), Galileo (E).
- Jamming, spoofing, and interference detection.
- RINEX 2.x, 3.x, 4.x observation file parsing.
- BRDC broadcast ephemeris parsing for azimuth/elevation computation.
- Anomaly detection with configurable sensitivity threshold.
- `to_json()` full result serialization.
- PyO3 Python bindings with zero-copy result access.
