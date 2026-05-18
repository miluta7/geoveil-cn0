# Changelog

## [0.3.7] - 2026-05-18

### Fixed
- Panic on empty satellite ID in RINEX parser (cn0.rs line 180).
  sat_id[1..] panicked on malformed records with empty satellite ID string.
  Fixed with sat_id.get(1..).unwrap_or("") which is bounds-safe.

## [0.3.6] - 2026-04-26

### Changed
- manylinux compatibility improvements for Linux wheel builds

## [0.3.5] and earlier

### Added
- Multi-GNSS CN0 analysis: GPS, GLONASS, Galileo, BeiDou, QZSS
- Jamming, spoofing and interference detection
- Skyplot data with azimuth/elevation from BRDC/SP3 ephemeris
- RINEX 2.x, 3.x, 4.x support with Hatanaka decompression
- Anomaly detection with configurable sensitivity
