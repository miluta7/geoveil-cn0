//! Navigation file parser and satellite position computation
//!
//! Supports BRDC navigation files for GPS, GLONASS, Galileo, BeiDou

use crate::types::*;
use crate::{GM_WGS84, OMEGA_EARTH, GM_GLO, RE_GLO, J2_GLO, OMEGA_GLO};
use std::collections::HashMap;

/// GPS/Galileo/BeiDou Keplerian ephemeris
#[derive(Debug, Clone, Default)]
pub struct KeplerianEphemeris {
    pub prn: u32,
    pub system: char,
    pub toc: Epoch,
    pub af0: f64,
    pub af1: f64,
    pub af2: f64,
    pub iode: f64,
    pub crs: f64,
    pub delta_n: f64,
    pub m0: f64,
    pub cuc: f64,
    pub e: f64,
    pub cus: f64,
    pub sqrt_a: f64,
    pub toe: f64,
    pub cic: f64,
    pub omega0: f64,
    pub cis: f64,
    pub i0: f64,
    pub crc: f64,
    pub omega: f64,
    pub omega_dot: f64,
    pub idot: f64,
    pub week: i32,
}

/// GLONASS state vector ephemeris
#[derive(Debug, Clone, Default)]
pub struct GlonassEphemeris {
    pub prn: u32,
    pub toc: Epoch,
    pub tau_n: f64,
    pub gamma_n: f64,
    pub tk: f64,
    pub x: f64,
    pub vx: f64,
    pub ax: f64,
    pub y: f64,
    pub vy: f64,
    pub ay: f64,
    pub z: f64,
    pub vz: f64,
    pub az: f64,
    pub fcn: i32,
}

/// Navigation data container
#[derive(Debug, Clone, Default)]
pub struct NavigationData {
    /// KEY IS STRING (satellite ID like "G01")
    pub gps_ephemeris: HashMap<String, Vec<KeplerianEphemeris>>,
    pub galileo_ephemeris: HashMap<String, Vec<KeplerianEphemeris>>,
    pub beidou_ephemeris: HashMap<String, Vec<KeplerianEphemeris>>,
    pub glonass_ephemeris: HashMap<String, Vec<GlonassEphemeris>>,
    /// KEY IS STRING (PRN as string)
    pub glonass_fcn: HashMap<String, i32>,
}

impl NavigationData {
    pub fn has_data(&self) -> bool {
        !self.gps_ephemeris.is_empty() ||
        !self.galileo_ephemeris.is_empty() ||
        !self.beidou_ephemeris.is_empty() ||
        !self.glonass_ephemeris.is_empty()
    }
    
    pub fn satellite_count(&self) -> usize {
        self.gps_ephemeris.len() +
        self.galileo_ephemeris.len() +
        self.beidou_ephemeris.len() +
        self.glonass_ephemeris.len()
    }
}

/// Parse navigation file
pub fn parse_navigation(content: &[u8]) -> Result<NavigationData, String> {
    let text = String::from_utf8_lossy(content);
    let lines: Vec<&str> = text.lines().collect();
    
    let mut nav_data = NavigationData::default();
    let mut header_end = 0;
    let mut nav_version = 3.0f64;
    
    // Parse header
    for (i, line) in lines.iter().enumerate() {
        if line.len() < 60 {
            continue;
        }
        
        let label = &line[60.min(line.len())..].trim();
        
        if label.contains("RINEX VERSION") {
            if let Ok(ver) = line[0..9].trim().parse::<f64>() {
                nav_version = ver;
            }
        } else if label.contains("END OF HEADER") {
            header_end = i + 1;
            break;
        }
    }
    
    if header_end == 0 {
        return Err("No END OF HEADER found".to_string());
    }
    
    // Parse ephemeris records
    let eph_lines = &lines[header_end..];
    let mut i = 0;
    
    while i < eph_lines.len() {
        let line = eph_lines[i];
        
        if line.trim().is_empty() {
            i += 1;
            continue;
        }
        
        // Determine system from first character (RINEX 3.x) or from file type
        let sys_char = if nav_version >= 3.0 {
            line.chars().next().unwrap_or('G')
        } else {
            'G' // RINEX 2.x GPS nav
        };
        
        match sys_char {
            'G' | 'E' | 'C' | 'J' => {
                // Keplerian ephemeris (GPS, Galileo, BeiDou, QZSS)
                if i + 7 < eph_lines.len() {
                    if let Some(eph) = parse_keplerian_ephemeris(&eph_lines[i..i+8], sys_char, nav_version) {
                        let sat_id = format!("{}{:02}", sys_char, eph.prn);
                        let map = match sys_char {
                            'G' | 'J' => &mut nav_data.gps_ephemeris,
                            'E' => &mut nav_data.galileo_ephemeris,
                            'C' => &mut nav_data.beidou_ephemeris,
                            _ => &mut nav_data.gps_ephemeris,
                        };
                        map.entry(sat_id).or_insert_with(Vec::new).push(eph);
                    }
                    i += 8;
                } else {
                    i += 1;
                }
            }
            'R' => {
                // GLONASS state vector ephemeris
                if i + 3 < eph_lines.len() {
                    if let Some(eph) = parse_glonass_ephemeris(&eph_lines[i..i+4]) {
                        let sat_id = format!("R{:02}", eph.prn);
                        if eph.fcn != 0 {
                            nav_data.glonass_fcn.insert(eph.prn.to_string(), eph.fcn);
                        }
                        nav_data.glonass_ephemeris.entry(sat_id).or_insert_with(Vec::new).push(eph);
                    }
                    i += 4;
                } else {
                    i += 1;
                }
            }
            _ => {
                i += 1;
            }
        }
    }
    
    Ok(nav_data)
}

fn parse_keplerian_ephemeris(lines: &[&str], sys: char, version: f64) -> Option<KeplerianEphemeris> {
    if lines.len() < 8 {
        return None;
    }
    
    let line0 = lines[0];
    
    // Parse PRN and time of clock
    let (prn, toc) = if version >= 3.0 {
        // RINEX 3.x: "G01 2024 06 15 12 00 00..."
        let prn: u32 = line0[1..3].trim().parse().ok()?;
        let toc = Epoch {
            year: line0[4..8].trim().parse().ok()?,
            month: line0[9..11].trim().parse().ok()?,
            day: line0[12..14].trim().parse().ok()?,
            hour: line0[15..17].trim().parse().ok()?,
            minute: line0[18..20].trim().parse().ok()?,
            second: line0[21..23].trim().parse().ok()?,
        };
        (prn, toc)
    } else {
        // RINEX 2.x: " 1 24  6 15 12  0  0.0..."
        let prn: u32 = line0[0..2].trim().parse().ok()?;
        let year_2d: i32 = line0[3..5].trim().parse().ok()?;
        let year = if year_2d >= 80 { 1900 + year_2d } else { 2000 + year_2d };
        let toc = Epoch {
            year,
            month: line0[6..8].trim().parse().ok()?,
            day: line0[9..11].trim().parse().ok()?,
            hour: line0[12..14].trim().parse().ok()?,
            minute: line0[15..17].trim().parse().ok()?,
            second: line0[17..22].trim().parse().ok()?,
        };
        (prn, toc)
    };
    
    // Helper to parse D-format scientific notation
    let parse_d = |s: &str| -> f64 {
        s.trim().replace('D', "E").replace('d', "e").parse().unwrap_or(0.0)
    };
    
    // Parse clock parameters from line 0
    let af0 = parse_d(&line0[23.min(line0.len())..42.min(line0.len())]);
    let af1 = parse_d(&line0[42.min(line0.len())..61.min(line0.len())]);
    let af2 = parse_d(&line0[61.min(line0.len())..80.min(line0.len())]);
    
    // Parse broadcast orbit parameters
    let get_field = |line: &str, start: usize| -> f64 {
        if line.len() > start {
            parse_d(&line[start.min(line.len())..(start + 19).min(line.len())])
        } else {
            0.0
        }
    };
    
    let l1 = lines[1];
    let l2 = lines[2];
    let l3 = lines[3];
    let l4 = lines[4];
    let l5 = lines[5];
    let _l6 = lines[6];  // Reserved for additional fields
    
    let offset = if version >= 3.0 { 4 } else { 3 };
    
    Some(KeplerianEphemeris {
        prn,
        system: sys,
        toc,
        af0,
        af1,
        af2,
        iode: get_field(l1, offset),
        crs: get_field(l1, offset + 19),
        delta_n: get_field(l1, offset + 38),
        m0: get_field(l1, offset + 57),
        cuc: get_field(l2, offset),
        e: get_field(l2, offset + 19),
        cus: get_field(l2, offset + 38),
        sqrt_a: get_field(l2, offset + 57),
        toe: get_field(l3, offset),
        cic: get_field(l3, offset + 19),
        omega0: get_field(l3, offset + 38),
        cis: get_field(l3, offset + 57),
        i0: get_field(l4, offset),
        crc: get_field(l4, offset + 19),
        omega: get_field(l4, offset + 38),
        omega_dot: get_field(l4, offset + 57),
        idot: get_field(l5, offset),
        week: get_field(l5, offset + 38) as i32,
    })
}

fn parse_glonass_ephemeris(lines: &[&str]) -> Option<GlonassEphemeris> {
    if lines.len() < 4 {
        return None;
    }
    
    let line0 = lines[0];
    
    // Parse PRN and time
    let prn: u32 = line0[1..3].trim().parse().ok()?;
    let toc = Epoch {
        year: line0[4..8].trim().parse().ok()?,
        month: line0[9..11].trim().parse().ok()?,
        day: line0[12..14].trim().parse().ok()?,
        hour: line0[15..17].trim().parse().ok()?,
        minute: line0[18..20].trim().parse().ok()?,
        second: line0[21..23].trim().parse().ok()?,
    };
    
    let parse_d = |s: &str| -> f64 {
        s.trim().replace('D', "E").replace('d', "e").parse().unwrap_or(0.0)
    };
    
    let get_field = |line: &str, start: usize| -> f64 {
        if line.len() > start {
            parse_d(&line[start.min(line.len())..(start + 19).min(line.len())])
        } else {
            0.0
        }
    };
    
    let l1 = lines[1];
    let l2 = lines[2];
    let l3 = lines[3];
    
    Some(GlonassEphemeris {
        prn,
        toc,
        tau_n: parse_d(&line0[23.min(line0.len())..42.min(line0.len())]),
        gamma_n: parse_d(&line0[42.min(line0.len())..61.min(line0.len())]),
        tk: parse_d(&line0[61.min(line0.len())..80.min(line0.len())]),
        x: get_field(l1, 4) * 1000.0,  // km to m
        vx: get_field(l1, 23) * 1000.0,
        ax: get_field(l1, 42) * 1000.0,
        y: get_field(l2, 4) * 1000.0,
        vy: get_field(l2, 23) * 1000.0,
        ay: get_field(l2, 42) * 1000.0,
        z: get_field(l3, 4) * 1000.0,
        vz: get_field(l3, 23) * 1000.0,
        az: get_field(l3, 42) * 1000.0,
        fcn: get_field(l2, 61) as i32,
    })
}

/// Compute satellite position at given epoch
pub fn compute_satellite_position(
    nav_data: &NavigationData,
    sat_id: &str,
    epoch: &Epoch,
) -> Option<Ecef> {
    let sys = sat_id.chars().next()?;
    
    match sys {
        'G' | 'J' => {
            let ephs = nav_data.gps_ephemeris.get(sat_id)?;
            let eph = find_closest_ephemeris(ephs, epoch)?;
            compute_keplerian_position(eph, epoch, GM_WGS84, OMEGA_EARTH)
        }
        'E' => {
            let ephs = nav_data.galileo_ephemeris.get(sat_id)?;
            let eph = find_closest_ephemeris(ephs, epoch)?;
            compute_keplerian_position(eph, epoch, GM_WGS84, OMEGA_EARTH)
        }
        'C' => {
            let ephs = nav_data.beidou_ephemeris.get(sat_id)?;
            let eph = find_closest_ephemeris(ephs, epoch)?;
            // BeiDou uses CGCS2000 which is very close to WGS84
            compute_keplerian_position(eph, epoch, GM_WGS84, OMEGA_EARTH)
        }
        'R' => {
            let ephs = nav_data.glonass_ephemeris.get(sat_id)?;
            let eph = find_closest_glonass_ephemeris(ephs, epoch)?;
            compute_glonass_position(eph, epoch)
        }
        _ => None,
    }
}

fn find_closest_ephemeris<'a>(ephs: &'a [KeplerianEphemeris], epoch: &Epoch) -> Option<&'a KeplerianEphemeris> {
    if ephs.is_empty() {
        return None;
    }
    
    let target_jd = epoch.julian_date();
    
    // Find closest ephemeris within validity window (±4 hours for GPS/Galileo/BeiDou)
    let max_age_days = 4.0 / 24.0;  // 4 hours in days
    
    ephs.iter()
        .filter(|e| (e.toc.julian_date() - target_jd).abs() < max_age_days)
        .min_by(|a, b| {
            let diff_a = (a.toc.julian_date() - target_jd).abs();
            let diff_b = (b.toc.julian_date() - target_jd).abs();
            diff_a.partial_cmp(&diff_b).unwrap_or(std::cmp::Ordering::Equal)
        })
}

fn find_closest_glonass_ephemeris<'a>(ephs: &'a [GlonassEphemeris], epoch: &Epoch) -> Option<&'a GlonassEphemeris> {
    if ephs.is_empty() {
        return None;
    }
    
    let target_jd = epoch.julian_date();
    
    // GLONASS ephemerides are valid for ~30 minutes typically
    let max_age_days = 1.0 / 24.0;  // 1 hour in days
    
    ephs.iter()
        .filter(|e| (e.toc.julian_date() - target_jd).abs() < max_age_days)
        .min_by(|a, b| {
            let diff_a = (a.toc.julian_date() - target_jd).abs();
            let diff_b = (b.toc.julian_date() - target_jd).abs();
            diff_a.partial_cmp(&diff_b).unwrap_or(std::cmp::Ordering::Equal)
        })
}

fn compute_keplerian_position(eph: &KeplerianEphemeris, epoch: &Epoch, gm: f64, omega_e: f64) -> Option<Ecef> {
    let a = eph.sqrt_a * eph.sqrt_a;
    let n0 = (gm / (a * a * a)).sqrt();
    let n = n0 + eph.delta_n;
    
    // Time from ephemeris reference epoch
    let (_, _toe_tow) = eph.toc.to_gps_time();
    let (_, t_tow) = epoch.to_gps_time();
    let mut dt = t_tow - eph.toe;
    
    // Week rollover
    if dt > 302400.0 {
        dt -= 604800.0;
    } else if dt < -302400.0 {
        dt += 604800.0;
    }
    
    // Mean anomaly
    let m = eph.m0 + n * dt;
    
    // Eccentric anomaly (Newton-Raphson)
    let mut e_anom = m;
    for _ in 0..10 {
        let f = e_anom - eph.e * e_anom.sin() - m;
        let fp = 1.0 - eph.e * e_anom.cos();
        e_anom -= f / fp;
    }
    
    // True anomaly
    let sin_e = e_anom.sin();
    let cos_e = e_anom.cos();
    let v = ((1.0 - eph.e * eph.e).sqrt() * sin_e).atan2(cos_e - eph.e);
    
    // Argument of latitude
    let phi = v + eph.omega;
    let sin_2phi = (2.0 * phi).sin();
    let cos_2phi = (2.0 * phi).cos();
    
    // Corrections
    let du = eph.cus * sin_2phi + eph.cuc * cos_2phi;
    let dr = eph.crs * sin_2phi + eph.crc * cos_2phi;
    let di = eph.cis * sin_2phi + eph.cic * cos_2phi;
    
    let u = phi + du;
    let r = a * (1.0 - eph.e * cos_e) + dr;
    let i = eph.i0 + eph.idot * dt + di;
    
    // Positions in orbital plane
    let x_op = r * u.cos();
    let y_op = r * u.sin();
    
    // Corrected longitude of ascending node
    let omega = eph.omega0 + (eph.omega_dot - omega_e) * dt - omega_e * eph.toe;
    
    // ECEF coordinates
    let cos_omega = omega.cos();
    let sin_omega = omega.sin();
    let cos_i = i.cos();
    let sin_i = i.sin();
    
    let x = x_op * cos_omega - y_op * cos_i * sin_omega;
    let y = x_op * sin_omega + y_op * cos_i * cos_omega;
    let z = y_op * sin_i;
    
    Some(Ecef::new(x, y, z))
}

fn compute_glonass_position(eph: &GlonassEphemeris, epoch: &Epoch) -> Option<Ecef> {
    // Time difference from ephemeris epoch
    let dt = epoch.diff_seconds(&eph.toc);
    
    if dt.abs() > 7200.0 {
        return None; // Ephemeris too old
    }
    
    // Initial state vector
    let mut state = [
        eph.x, eph.y, eph.z,
        eph.vx, eph.vy, eph.vz,
    ];
    let accel = [eph.ax, eph.ay, eph.az];
    
    // RK4 integration
    let step: f64 = if dt > 0.0 { 60.0 } else { -60.0 };
    let mut t: f64 = 0.0;
    
    while t.abs() < dt.abs() {
        let h = if (dt - t).abs() < step.abs() {
            dt - t
        } else {
            step
        };
        
        rk4_step(&mut state, &accel, h);
        t += h;
    }
    
    Some(Ecef::new(state[0], state[1], state[2]))
}

fn rk4_step(state: &mut [f64; 6], accel: &[f64; 3], h: f64) {
    let k1 = glonass_derivatives(state, accel);
    
    let mut s2 = *state;
    for i in 0..6 {
        s2[i] += h * 0.5 * k1[i];
    }
    let k2 = glonass_derivatives(&s2, accel);
    
    let mut s3 = *state;
    for i in 0..6 {
        s3[i] += h * 0.5 * k2[i];
    }
    let k3 = glonass_derivatives(&s3, accel);
    
    let mut s4 = *state;
    for i in 0..6 {
        s4[i] += h * k3[i];
    }
    let k4 = glonass_derivatives(&s4, accel);
    
    for i in 0..6 {
        state[i] += h * (k1[i] + 2.0 * k2[i] + 2.0 * k3[i] + k4[i]) / 6.0;
    }
}

fn glonass_derivatives(state: &[f64; 6], accel: &[f64; 3]) -> [f64; 6] {
    let x = state[0];
    let y = state[1];
    let z = state[2];
    let vx = state[3];
    let vy = state[4];
    let vz = state[5];
    
    let r = (x * x + y * y + z * z).sqrt();
    let r2 = r * r;
    let r3 = r2 * r;
    // r5 and rho reserved for higher-order perturbations
    let _r5 = r3 * r2;
    
    let rho2 = x * x + y * y;
    let _rho = rho2.sqrt();
    
    // J2 perturbation
    let c = -GM_GLO / r3;
    let j2_factor = 1.5 * J2_GLO * RE_GLO * RE_GLO / r2;
    let z2_r2 = z * z / r2;
    
    let ax = c * x * (1.0 + j2_factor * (1.0 - 5.0 * z2_r2))
           + OMEGA_GLO * OMEGA_GLO * x
           + 2.0 * OMEGA_GLO * vy
           + accel[0];
           
    let ay = c * y * (1.0 + j2_factor * (1.0 - 5.0 * z2_r2))
           + OMEGA_GLO * OMEGA_GLO * y
           - 2.0 * OMEGA_GLO * vx
           + accel[1];
           
    let az = c * z * (1.0 + j2_factor * (3.0 - 5.0 * z2_r2))
           + accel[2];
    
    [vx, vy, vz, ax, ay, az]
}

/// Calculate azimuth and elevation from receiver to satellite
pub fn calculate_azel(receiver: &Ecef, satellite: &Ecef) -> (f64, f64) {
    // Get receiver geodetic position
    let (lat, lon, _) = receiver.to_geodetic();
    let lat_rad = lat.to_radians();
    let lon_rad = lon.to_radians();
    
    // Line-of-sight vector in ECEF
    let dx = satellite.x - receiver.x;
    let dy = satellite.y - receiver.y;
    let dz = satellite.z - receiver.z;
    
    // Rotation matrix to ENU
    let sin_lat = lat_rad.sin();
    let cos_lat = lat_rad.cos();
    let sin_lon = lon_rad.sin();
    let cos_lon = lon_rad.cos();
    
    // ENU coordinates
    let e = -sin_lon * dx + cos_lon * dy;
    let n = -sin_lat * cos_lon * dx - sin_lat * sin_lon * dy + cos_lat * dz;
    let u = cos_lat * cos_lon * dx + cos_lat * sin_lon * dy + sin_lat * dz;
    
    // Azimuth (from North, clockwise)
    let az = e.atan2(n).to_degrees();
    let az = if az < 0.0 { az + 360.0 } else { az };
    
    // Elevation
    let horiz = (e * e + n * n).sqrt();
    let el = u.atan2(horiz).to_degrees();
    
    (az, el)
}
