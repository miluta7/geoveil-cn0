//! Plausibility of the epoch times themselves.
//!
//! A receiver that follows a false time source, or resolves the GPS week 1024 weeks late,
//! still writes well-formed RINEX. Two things give it away: data that claims to be observed
//! after the file was written (or in the future), and, for a week error, an offset that is a
//! whole number of 1024-week cycles. Data dated long *before* the file was written is normal
//! (archives get re-exported), so that direction is never flagged. Found in the 2026 Sulina
//! time-purity study: 264 files dated exactly 1024 weeks late, 4 stamped 10 March 2028.

use crate::types::{Epoch, RinexObsData, TimeIntegrity};

/// GPS week rollover period
const ROLLOVER_WEEKS: f64 = 1024.0;
/// Still one rollover when the write date trails the observations by up to a year (re-exports)
const ROLLOVER_TOLERANCE_WEEKS: f64 = 52.0;
/// A live file's header is written as recording starts; its first epoch may follow by a day
const AFTER_WRITTEN_SLACK_DAYS: f64 = 2.0;

/// Assess the epoch times of `obs`. `now_jd` is the current time as a Julian date.
pub fn assess(obs: &RinexObsData, now_jd: f64) -> TimeIntegrity {
    let mut t = TimeIntegrity {
        file_written: obs.file_written.map(|e| e.to_iso_string()),
        event_records: obs.event_records,
        power_failure_epochs: obs.epochs.iter().filter(|e| e.flag == 1).count(),
        ..Default::default()
    };
    let Some(first) = obs.epochs.first().map(|e| e.epoch) else {
        t.verdict = "No epochs".into();
        return t;
    };

    let interval = if obs.interval > 0.0 { obs.interval } else { median_step(obs) };
    for w in obs.epochs.windows(2) {
        let dt = w[1].epoch.diff_seconds(&w[0].epoch);
        if dt < -1e-3 {
            t.backward_epochs += 1;
        } else if dt <= 1e-3 {
            t.repeated_epochs += 1;
        } else if interval > 0.0 && dt > 1.5 * interval {
            t.gaps += 1;
        }
    }

    // Reference: when the file was written, unless that date is missing or itself impossible
    let written = obs.file_written.map(|e| e.julian_date()).filter(|&jd| jd > jd_of(1995, 1, 1) && jd < now_jd + 1.0);
    let first_jd = first.julian_date();
    let ahead_days = first_jd - written.unwrap_or(now_jd);
    t.weeks_after_reference = Some(ahead_days / 7.0);
    t.implausible_date = ahead_days > if written.is_some() { AFTER_WRITTEN_SLACK_DAYS } else { 1.0 }
        || first_jd > now_jd + 1.0;
    if t.implausible_date {
        let weeks = ahead_days / 7.0;
        let cycles = (weeks / ROLLOVER_WEEKS).round();
        if cycles >= 1.0 && (weeks - cycles * ROLLOVER_WEEKS).abs() <= ROLLOVER_TOLERANCE_WEEKS {
            t.week_rollover_suspected = true;
            t.rollover_cycles = cycles as i32;
        }
    }

    let observed = |sys: char| obs.epochs.iter().any(|e| e.observations.keys().any(|s| s.starts_with(sys)));
    t.glonass_declared_not_observed = obs.obs_types.contains_key("R") && !observed('R');

    t.verdict = if t.week_rollover_suspected {
        format!("Dated {} x 1024 GPS weeks late: genuine signals, wrong receiver week. Re-date by subtracting {} weeks",
                t.rollover_cycles, t.rollover_cycles * 1024)
    } else if t.implausible_date {
        format!("Dated {:.0} weeks after the file was written{}, not a GPS week rollover: the receiver followed a false time source",
                ahead_days / 7.0, if written.is_some() { "" } else { " (or ahead of now)" })
    } else if t.backward_epochs + t.repeated_epochs > 0 {
        "Epochs go backwards or repeat".into()
    } else {
        "OK".into()
    };
    t
}

fn jd_of(y: i32, m: u32, d: u32) -> f64 {
    Epoch::new(y, m, d, 0, 0, 0.0).julian_date()
}

fn median_step(obs: &RinexObsData) -> f64 {
    let mut steps: Vec<f64> = obs.epochs.windows(2).map(|w| w[1].epoch.diff_seconds(&w[0].epoch)).filter(|d| *d > 0.0).collect();
    if steps.is_empty() {
        return 0.0;
    }
    steps.sort_by(|a, b| a.partial_cmp(b).unwrap());
    steps[steps.len() / 2]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{EpochData, ObsValue, SatelliteObs};
    use std::collections::HashMap;

    const NOW: (i32, u32, u32) = (2026, 11, 27);

    fn file(written: Option<Epoch>, epochs: &[Epoch], systems: &[&str], sats: &[&str]) -> RinexObsData {
        let obs: HashMap<String, SatelliteObs> = sats.iter().map(|s| {
            let mut o = SatelliteObs::default();
            o.values.insert("S1C".into(), ObsValue { value: 45.0, lli: None, ssi: None });
            (s.to_string(), o)
        }).collect();
        RinexObsData {
            interval: 1.0,
            file_written: written,
            obs_types: systems.iter().map(|s| (s.to_string(), vec!["S1C".to_string()])).collect(),
            epochs: epochs.iter().map(|e| EpochData { epoch: *e, flag: 0, observations: obs.clone() }).collect(),
            ..Default::default()
        }
    }

    fn now() -> f64 {
        jd_of(NOW.0, NOW.1, NOW.2)
    }

    #[test]
    fn sulina_2045_file_is_a_week_rollover() {
        // Header written 18 March 2026, first epoch 1 November 2045; GPS, Galileo, BeiDou, no GLONASS
        let o = file(Some(Epoch::new(2026, 3, 18, 17, 3, 47.0)),
                     &[Epoch::new(2045, 11, 1, 14, 49, 57.0), Epoch::new(2045, 11, 1, 14, 49, 58.0)],
                     &["G", "R", "E", "C"], &["G05", "E11", "C23"]);
        let t = assess(&o, now());
        assert!(t.implausible_date && t.week_rollover_suspected, "{t:?}");
        assert_eq!(t.rollover_cycles, 1);
        assert!(t.glonass_declared_not_observed);
        assert!(t.verdict.contains("1024 weeks"), "{}", t.verdict);
    }

    #[test]
    fn sulina_2028_file_is_a_false_time_source() {
        // Header written 17 June 2026, data stamped 10 March 2028: 90 weeks, not a rollover
        let o = file(Some(Epoch::new(2026, 6, 17, 15, 1, 51.0)),
                     &[Epoch::new(2028, 3, 10, 4, 1, 43.0)], &["G", "R"], &["G05"]);
        let t = assess(&o, now());
        assert!(t.implausible_date && !t.week_rollover_suspected, "{t:?}");
        assert!(t.verdict.contains("false time source"), "{}", t.verdict);
    }

    #[test]
    fn normal_and_reexported_files_pass() {
        // Live file: header written at the first epoch
        let live = file(Some(Epoch::new(2026, 11, 26, 0, 0, 0.0)),
                        &[Epoch::new(2026, 11, 26, 0, 0, 0.0), Epoch::new(2026, 11, 26, 0, 0, 1.0)], &["G", "R"], &["G05", "R07"]);
        let t = assess(&live, now());
        assert!(!t.implausible_date && !t.glonass_declared_not_observed && t.verdict == "OK", "{t:?}");
        // A 2007 archive re-exported in 2026 sits ~1024 weeks *before* its write date: fine
        let old = file(Some(Epoch::new(2026, 11, 26, 0, 0, 0.0)), &[Epoch::new(2007, 3, 1, 0, 0, 0.0)], &["G"], &["G05"]);
        assert!(!assess(&old, now()).implausible_date);
        // No write date: only the future counts
        let no_date = file(None, &[Epoch::new(2045, 11, 1, 0, 0, 0.0)], &["G"], &["G05"]);
        assert!(assess(&no_date, now()).week_rollover_suspected);
    }

    #[test]
    fn backward_repeated_epochs_and_gaps_are_counted() {
        let e = |s: f64| Epoch::new(2026, 11, 26, 0, 0, s);
        let o = file(Some(e(0.0)), &[e(0.0), e(1.0), e(1.0), e(0.5), e(10.0)], &["G"], &["G05"]);
        let t = assess(&o, now());
        assert_eq!((t.backward_epochs, t.repeated_epochs, t.gaps), (1, 1, 1), "{t:?}");
        assert_eq!(t.verdict, "Epochs go backwards or repeat");
    }
}
