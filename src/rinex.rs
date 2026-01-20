//! RINEX observation file parser
//!
//! Supports RINEX 2.x, 3.x, and 4.x formats

use crate::types::*;
use std::collections::HashMap;

/// Parse RINEX observation file from bytes
pub fn parse_rinex_obs(content: &[u8], filename: &str) -> Result<RinexObsData, String> {
    let text = String::from_utf8_lossy(content);
    let lines: Vec<&str> = text.lines().collect();
    
    let mut data = RinexObsData::default();
    let mut header_end = 0;
    
    let mut last_obs_system: Option<char> = None;
    
    // Parse header
    for (i, line) in lines.iter().enumerate() {
        if line.len() < 60 {
            continue;
        }
        
        let label = if line.len() >= 60 { &line[60..].trim() } else { "" };
        
        if label.contains("RINEX VERSION") {
            if let Ok(ver) = line[0..9].trim().parse::<f64>() {
                data.version = ver;
            }
        } else if label.contains("MARKER NAME") {
            data.marker_name = line[0..60.min(line.len())].trim().to_string();
        } else if label.contains("REC # / TYPE") {
            if line.len() >= 40 {
                data.receiver_type = line[20..40.min(line.len())].trim().to_string();
            }
        } else if label.contains("ANT # / TYPE") {
            if line.len() >= 40 {
                data.antenna_type = line[20..40.min(line.len())].trim().to_string();
            }
        } else if label.contains("APPROX POSITION XYZ") {
            let parts: Vec<f64> = line[0..60.min(line.len())]
                .split_whitespace()
                .filter_map(|s| s.parse().ok())
                .collect();
            if parts.len() >= 3 {
                data.approx_position = Some(Ecef::new(parts[0], parts[1], parts[2]));
            }
        } else if label.contains("INTERVAL") {
            if let Ok(interval) = line[0..10].trim().parse::<f64>() {
                data.interval = interval;
            }
        } else if label.contains("SYS / # / OBS TYPES") {
            // RINEX 3.x+ format - pass and update last_obs_system for continuation
            last_obs_system = parse_obs_types_v3(line, &mut data.obs_types, last_obs_system);
        } else if label.contains("# / TYPES OF OBSERV") {
            // RINEX 2.x format
            parse_obs_types_v2(line, &mut data.obs_types);
        } else if label.contains("GLONASS SLOT / FRQ") || label.contains("GLONASS COD/PHS/BIS") {
            parse_glonass_fcn(line, &mut data.glonass_fcn);
        } else if label.contains("END OF HEADER") {
            header_end = i + 1;
            break;
        }
    }
    
    if header_end == 0 {
        return Err("No END OF HEADER found".to_string());
    }
    
    // Infer filename for marker if empty
    if data.marker_name.is_empty() {
        data.marker_name = filename.to_string();
    }
    
    // Parse epochs
    let epoch_lines = &lines[header_end..];
    
    if data.version >= 3.0 {
        parse_epochs_v3(epoch_lines, &mut data)?;
    } else {
        parse_epochs_v2(epoch_lines, &mut data)?;
    }
    
    Ok(data)
}

fn parse_obs_types_v3(line: &str, obs_types: &mut HashMap<String, Vec<String>>, last_system: Option<char>) -> Option<char> {
    if line.len() < 7 {
        return last_system;
    }
    
    let first_char = line.chars().next().unwrap_or(' ');
    
    // Determine the system: either from this line or continuation of previous
    let sys_char = if first_char != ' ' && "GREJCIS".contains(first_char) {
        // New system line
        first_char
    } else {
        // Continuation line - use last system
        match last_system {
            Some(c) => c,
            None => return last_system,  // Can't parse without knowing system
        }
    };
    
    let sys_str = sys_char.to_string();
    
    // Parse observation types
    // RINEX 3.x format: column 1 = system, columns 4-6 = count, columns 7+ = obs types
    // Continuation lines have spaces in columns 1-6, obs types start at column 7
    let types_part = if line.len() > 6 { 
        &line[6..60.min(line.len())] 
    } else { 
        "" 
    };
    
    let types: Vec<String> = types_part
        .split_whitespace()
        .filter(|s| s.len() >= 3 && s.chars().next().map(|c| c.is_alphabetic()).unwrap_or(false))
        .map(|s| s.to_string())
        .collect();
    
    if !types.is_empty() {
        let entry = obs_types.entry(sys_str).or_insert_with(Vec::new);
        entry.extend(types);
    }
    
    Some(sys_char)
}

fn parse_obs_types_v2(line: &str, obs_types: &mut HashMap<String, Vec<String>>) {
    // RINEX 2.x uses same obs types for all systems
    let types: Vec<String> = line[0..60.min(line.len())]
        .split_whitespace()
        .filter(|s| s.len() >= 2 && !s.chars().next().unwrap_or(' ').is_digit(10))
        .map(|s| {
            // Convert RINEX 2.x codes to 3.x style
            match s {
                "C1" => "C1C".to_string(),
                "P1" => "C1P".to_string(),
                "L1" => "L1C".to_string(),
                "S1" => "S1C".to_string(),
                "C2" => "C2C".to_string(),
                "P2" => "C2P".to_string(),
                "L2" => "L2C".to_string(),
                "S2" => "S2C".to_string(),
                "C5" => "C5Q".to_string(),
                "L5" => "L5Q".to_string(),
                "S5" => "S5Q".to_string(),
                _ => s.to_string(),
            }
        })
        .collect();
    
    if !types.is_empty() {
        // Apply to GPS, GLONASS, Galileo, BeiDou
        for sys in &["G", "R", "E", "C"] {
            obs_types.insert(sys.to_string(), types.clone());
        }
    }
}

fn parse_glonass_fcn(line: &str, fcn: &mut HashMap<String, i32>) {
    // Parse "R01  1 R02 -4 R03  5 ..." format
    let content = &line[0..60.min(line.len())];
    let parts: Vec<&str> = content.split_whitespace().collect();
    
    let mut i = 0;
    while i + 1 < parts.len() {
        if parts[i].starts_with('R') && parts[i].len() >= 3 {
            if let (Ok(prn), Ok(slot)) = (
                parts[i][1..].parse::<u32>(),
                parts[i + 1].parse::<i32>()
            ) {
                fcn.insert(prn.to_string(), slot);
                i += 2;
                continue;
            }
        }
        i += 1;
    }
}

fn parse_epochs_v3(lines: &[&str], data: &mut RinexObsData) -> Result<(), String> {
    let mut i = 0;
    
    while i < lines.len() {
        let line = lines[i];
        
        // Skip empty lines and comment lines
        if line.trim().is_empty() || line.starts_with('%') || line.starts_with('#') {
            i += 1;
            continue;
        }
        
        // Epoch line starts with ">"
        if !line.starts_with('>') {
            i += 1;
            continue;
        }
        
        // Parse epoch header: > 2024 06 15 12 30 45.0000000  0 12
        let epoch = match parse_epoch_time_v3(&line[1..]) {
            Ok(e) => e,
            Err(_) => {
                // Skip malformed epoch lines instead of failing
                i += 1;
                continue;
            }
        };
        
        // Get flag and satellite count
        let parts: Vec<&str> = line[1..].split_whitespace().collect();
        let flag = if parts.len() > 6 {
            parts[6].parse::<u8>().unwrap_or(0)
        } else {
            0
        };
        let num_sats = if parts.len() > 7 {
            parts[7].parse::<usize>().unwrap_or(0)
        } else {
            // Try to count satellite lines dynamically
            0
        };
        
        let mut epoch_data = EpochData {
            epoch,
            flag,
            observations: HashMap::new(),
        };
        
        i += 1;
        
        // Read satellite observations
        for _ in 0..num_sats {
            if i >= lines.len() {
                break;
            }
            
            let sat_line = lines[i];
            if sat_line.is_empty() || sat_line.starts_with('>') {
                break;
            }
            
            if sat_line.len() >= 3 {
                let sat_id = sat_line[0..3].trim().to_string();
                let sys_char = sat_id.chars().next().unwrap_or('G');
                let sys_str = sys_char.to_string();
                
                let mut sat_obs = SatelliteObs::default();
                
                // Get obs types for this system
                let obs_type_list = data.obs_types.get(&sys_str).cloned().unwrap_or_default();
                
                // Parse observation values (start at position 3)
                let obs_part = if sat_line.len() > 3 { &sat_line[3..] } else { "" };
                
                for (j, obs_code) in obs_type_list.iter().enumerate() {
                    let start = j * 16;
                    let end = start + 14;
                    
                    if start >= obs_part.len() {
                        break;
                    }
                    
                    let val_str = if end <= obs_part.len() {
                        &obs_part[start..end]
                    } else if start < obs_part.len() {
                        &obs_part[start..]
                    } else {
                        ""
                    };
                    
                    if let Ok(value) = val_str.trim().parse::<f64>() {
                        if value != 0.0 {
                            // Parse LLI and SSI if present
                            let lli = if end + 1 <= obs_part.len() {
                                obs_part[end..end+1].trim().parse::<u8>().ok()
                            } else {
                                None
                            };
                            let ssi = if end + 2 <= obs_part.len() {
                                obs_part[end+1..end+2].trim().parse::<u8>().ok()
                            } else {
                                None
                            };
                            
                            sat_obs.values.insert(obs_code.clone(), ObsValue { value, lli, ssi });
                        }
                    }
                }
                
                if !sat_obs.values.is_empty() {
                    epoch_data.observations.insert(sat_id, sat_obs);
                }
            }
            
            i += 1;
        }
        
        if !epoch_data.observations.is_empty() {
            data.epochs.push(epoch_data);
        }
    }
    
    Ok(())
}

fn parse_epochs_v2(lines: &[&str], data: &mut RinexObsData) -> Result<(), String> {
    let mut i = 0;
    
    while i < lines.len() {
        let line = lines[i];
        
        // RINEX 2.x epoch line format: " 24  6 15 12 30 45.0000000  0 12G01G02..."
        if line.len() < 26 {
            i += 1;
            continue;
        }
        
        // Try to parse as epoch line
        let epoch = match parse_epoch_time_v2(line) {
            Ok(e) => e,
            Err(_) => {
                i += 1;
                continue;
            }
        };
        
        // Parse flag and number of satellites
        let flag = line[26..29].trim().parse::<u8>().unwrap_or(0);
        let num_sats = line[29..32].trim().parse::<usize>().unwrap_or(0);
        
        if num_sats == 0 {
            i += 1;
            continue;
        }
        
        // Parse satellite list (starting at position 32)
        let mut sat_list: Vec<String> = Vec::new();
        let mut sat_part = &line[32..];
        
        // May span multiple lines if > 12 satellites
        let mut remaining_sats = num_sats;
        while remaining_sats > 0 {
            let count = 12.min(remaining_sats);
            for j in 0..count {
                let start = j * 3;
                let end = start + 3;
                if end <= sat_part.len() {
                    let sat_id = sat_part[start..end].trim();
                    if !sat_id.is_empty() {
                        // Handle RINEX 2.x satellite IDs (might be " 1" instead of "G01")
                        let sat_id = if sat_id.len() <= 2 && sat_id.chars().all(|c| c.is_digit(10) || c == ' ') {
                            format!("G{:02}", sat_id.trim().parse::<u32>().unwrap_or(0))
                        } else if sat_id.len() == 3 {
                            sat_id.to_string()
                        } else {
                            format!("G{:02}", sat_id.trim().parse::<u32>().unwrap_or(0))
                        };
                        sat_list.push(sat_id);
                    }
                }
            }
            remaining_sats -= count;
            if remaining_sats > 0 {
                i += 1;
                if i >= lines.len() {
                    break;
                }
                sat_part = &lines[i][32.min(lines[i].len())..];
            }
        }
        
        i += 1;
        
        let mut epoch_data = EpochData {
            epoch,
            flag,
            observations: HashMap::new(),
        };
        
        // Get observation types (same for all systems in RINEX 2.x)
        let obs_types = data.obs_types.get("G").cloned().unwrap_or_default();
        let obs_per_line = 5;
        let lines_per_sat = (obs_types.len() + obs_per_line - 1) / obs_per_line;
        
        // Parse observations for each satellite
        for sat_id in &sat_list {
            let mut sat_obs = SatelliteObs::default();
            let mut obs_idx = 0;
            
            for _ in 0..lines_per_sat {
                if i >= lines.len() {
                    break;
                }
                
                let obs_line = lines[i];
                
                for j in 0..obs_per_line {
                    if obs_idx >= obs_types.len() {
                        break;
                    }
                    
                    let start = j * 16;
                    let end = start + 14;
                    
                    if start >= obs_line.len() {
                        obs_idx += 1;
                        continue;
                    }
                    
                    let val_str = if end <= obs_line.len() {
                        &obs_line[start..end]
                    } else if start < obs_line.len() {
                        &obs_line[start..]
                    } else {
                        ""
                    };
                    
                    if let Ok(value) = val_str.trim().parse::<f64>() {
                        if value != 0.0 {
                            let obs_code = &obs_types[obs_idx];
                            sat_obs.values.insert(obs_code.clone(), ObsValue { value, lli: None, ssi: None });
                        }
                    }
                    
                    obs_idx += 1;
                }
                
                i += 1;
            }
            
            if !sat_obs.values.is_empty() {
                epoch_data.observations.insert(sat_id.clone(), sat_obs);
            }
        }
        
        if !epoch_data.observations.is_empty() {
            data.epochs.push(epoch_data);
        }
    }
    
    Ok(())
}

fn parse_epoch_time_v3(s: &str) -> Result<Epoch, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("Empty epoch string".to_string());
    }
    
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() < 6 {
        return Err(format!("Invalid epoch format: expected 6+ parts, got {} in '{}'", parts.len(), s));
    }
    
    Ok(Epoch {
        year: parts[0].parse().map_err(|e| format!("Invalid year '{}': {}", parts[0], e))?,
        month: parts[1].parse().map_err(|e| format!("Invalid month '{}': {}", parts[1], e))?,
        day: parts[2].parse().map_err(|e| format!("Invalid day '{}': {}", parts[2], e))?,
        hour: parts[3].parse().map_err(|e| format!("Invalid hour '{}': {}", parts[3], e))?,
        minute: parts[4].parse().map_err(|e| format!("Invalid minute '{}': {}", parts[4], e))?,
        second: parts[5].parse().map_err(|e| format!("Invalid second '{}': {}", parts[5], e))?,
    })
}

fn parse_epoch_time_v2(s: &str) -> Result<Epoch, String> {
    if s.len() < 26 {
        return Err("Line too short".to_string());
    }
    
    let year_2d: i32 = s[1..3].trim().parse().map_err(|_| "Invalid year")?;
    let year = if year_2d >= 80 { 1900 + year_2d } else { 2000 + year_2d };
    
    Ok(Epoch {
        year,
        month: s[4..6].trim().parse().map_err(|_| "Invalid month")?,
        day: s[7..9].trim().parse().map_err(|_| "Invalid day")?,
        hour: s[10..12].trim().parse().map_err(|_| "Invalid hour")?,
        minute: s[13..15].trim().parse().map_err(|_| "Invalid minute")?,
        second: s[15..26].trim().parse().map_err(|_| "Invalid second")?,
    })
}
