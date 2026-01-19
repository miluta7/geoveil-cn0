# =============================================================================
# GeoVeil CN0 Analysis Widget v0.3.1
# =============================================================================
# Complete Jupyter notebook widget for GNSS CN0 signal quality analysis
#
# Features:
# - Research-based preset configurations (ITU, Stanford GPS Lab, GPS Solutions)
# - Lock Integrity metric (cycle slips + data gaps)
# - Separate output buttons for different views
# - Fixed anomaly graph using pd.to_datetime for robust timestamp parsing
# - Per-constellation satellite CN0 timeseries
# - Time vs Satellite heatmap
# - Auto-download BRDC navigation files
# - HTML report export
#
# Author: Miluta Dulea-Flueras
# Date: January 2025
# =============================================================================

import ipywidgets as widgets
from IPython.display import display, HTML, clear_output
import os
import gzip
import tempfile
from pathlib import Path
from datetime import datetime, date, timedelta
import urllib.request
import ssl
import json
import numpy as np
import pandas as pd
import plotly.graph_objects as go
from plotly.subplots import make_subplots

# Try to import the Rust library
try:
    import geoveil_cn0 as gcn0
    HAS_RUST_LIB = True
    LIB_VERSION = gcn0.VERSION
except ImportError:
    HAS_RUST_LIB = False
    LIB_VERSION = "Not installed"
    print("⚠️ geoveil_cn0 not installed. Install with: pip install geoveil-cn0")

# Storage for loaded data
loaded_data = {
    'obs_content': None,
    'obs_filename': None,
    'obs_path': None,
    'nav_content': None,
    'nav_filename': None,
    'nav_path': None,
}

# Store analysis results
analysis_results = {'data': None, 'figures': {}, 'report_html': '', 'lock_integrity': {}}

# ============================================================================
# PRESET CONFIGURATIONS (Research-based thresholds)
# ============================================================================
# Sources:
# - ITU-R M.1902-1: I/N = -6 dB (1 dB noise floor increase)
# - Stanford GPS Lab: CN0 min 27 dB-Hz, >6 dB drop in <3s = jamming
# - GPS Solutions: CN0 uniformity <2 dB std = spoofing indicator
# - MDPI Sensors: Multi-parameter detection

PRESET_CONFIGS = {
    'full': {
        'sensitivity': 0.3,
        'threshold': 8.0,
        'description': 'Complete analysis with all plots',
        'skip_heavy_plots': False,
    },
    'quick': {
        'sensitivity': 0.5,
        'threshold': 10.0,
        'description': 'Fast overview - skips heatmaps and per-satellite plots',
        'skip_heavy_plots': True,
    },
    'interference': {
        'sensitivity': 0.15,
        'threshold': 4.0,
        'description': 'Detect subtle interference >4 dB (ITU criterion)',
        'skip_heavy_plots': False,
    },
    'jamming': {
        'sensitivity': 0.2,
        'threshold': 6.0,
        'description': 'Optimized for jamming: rapid CN0 drops >6 dB',
        'skip_heavy_plots': False,
    },
    'spoofing': {
        'sensitivity': 0.1,
        'threshold': 5.0,
        'description': 'Focus on CN0 uniformity and elevation anomalies',
        'skip_heavy_plots': False,
    },
}


# ============================================================================
# NAVIGATION DOWNLOADER
# ============================================================================
class NavDownloader:
    """Multi-GNSS Navigation/Ephemeris Downloader - Smart Source Selection"""
    
    @staticmethod
    def gps_week_from_date(year, doy):
        """Calculate GPS week and day-of-week from year and DOY"""
        gps_epoch = date(1980, 1, 6)
        target = date(year, 1, 1) + timedelta(days=doy - 1)
        delta = (target - gps_epoch).days
        return delta // 7, delta % 7
    
    @staticmethod
    def parse_rinex_date(filename):
        """Extract year, doy from RINEX filename"""
        import re
        try:
            # RINEX 3/4 format: SSSSMMMMR_U_YYYYDDDHHMM_...
            parts = [p for p in filename.split('_') if p]
            if len(parts) >= 4 and len(parts[2]) >= 7:
                ts = parts[2]
                return int(ts[0:4]), int(ts[4:7])
            
            # RINEX 2 format: ssssdddf.yyt
            match = re.match(r'^[a-zA-Z0-9]{4}(\d{3})\d?\.(\d{2})[oOnNmMgG]$', filename)
            if match:
                doy = int(match.group(1))
                yr = int(match.group(2))
                year = 2000 + yr if yr < 80 else 1900 + yr
                return year, doy
            
            # Extended RINEX 2 format
            match = re.match(r'^[a-zA-Z0-9]{4}(\d{3})\d*\.(\d{2})[oOnNmMgG]$', filename)
            if match:
                doy = int(match.group(1))
                yr = int(match.group(2))
                year = 2000 + yr if yr < 80 else 1900 + yr
                return year, doy
                
        except Exception as e:
            print(f"   Date parse error: {e}")
        return None, None
    
    @staticmethod
    def parse_rinex_header(content):
        """Extract year, doy from RINEX file content header"""
        try:
            if isinstance(content, bytes):
                content = content.decode('utf-8', errors='ignore')
            
            for line in content.split('\n'):
                if 'TIME OF FIRST OBS' in line:
                    parts = line.split()
                    if len(parts) >= 6:
                        year = int(float(parts[0]))
                        month = int(float(parts[1]))
                        day = int(float(parts[2]))
                        doy = date(year, month, day).timetuple().tm_yday
                        return year, doy
                if 'END OF HEADER' in line:
                    break
        except Exception as e:
            print(f"   Header parse error: {e}")
        return None, None
    
    @staticmethod
    def count_nav_satellites(content):
        """Count satellites per constellation in navigation file content"""
        if isinstance(content, bytes):
            content = content.decode('utf-8', errors='ignore')
        
        sats = {'G': set(), 'R': set(), 'E': set(), 'C': set(), 'J': set(), 'I': set()}
        
        for line in content.split('\n'):
            if len(line) >= 3:
                first_char = line[0]
                if first_char in sats:
                    try:
                        prn = line[1:3].strip()
                        if prn.isdigit():
                            sats[first_char].add(int(prn))
                    except:
                        pass
        
        return {k: len(v) for k, v in sats.items()}
    
    @staticmethod
    def format_sat_summary(counts):
        """Format satellite count summary"""
        names = {'G': 'GPS', 'R': 'GLO', 'E': 'GAL', 'C': 'BDS', 'J': 'QZS', 'I': 'NAV'}
        parts = []
        for sys, count in counts.items():
            if count > 0:
                parts.append(f"{names.get(sys, sys)}:{count}")
        return ", ".join(parts)
    
    @staticmethod
    def download(year, doy, output_dir, log_func=print):
        """Download BRDC navigation file - tries multiple sources"""
        ctx = ssl.create_default_context()
        ctx.check_hostname = False
        ctx.verify_mode = ssl.CERT_NONE
        
        brdc_sources = [
            {"name": "BKG IGS", 
             "url": f"https://igs.bkg.bund.de/root_ftp/IGS/BRDC/{year}/{doy:03d}/BRDC00IGS_R_{year}{doy:03d}0000_01D_MN.rnx.gz",
             "filename": f"BRDC00IGS_R_{year}{doy:03d}0000_01D_MN.rnx"},
            {"name": "DLR MGEX", 
             "url": f"https://igs.bkg.bund.de/root_ftp/MGEX/BRDC/{year}/{doy:03d}/BRDM00DLR_S_{year}{doy:03d}0000_01D_MN.rnx.gz",
             "filename": f"BRDM00DLR_S_{year}{doy:03d}0000_01D_MN.rnx"},
            {"name": "IGN France", 
             "url": f"https://igs.ign.fr/pub/igs/data/{year}/{doy:03d}/BRDC00IGS_R_{year}{doy:03d}0000_01D_MN.rnx.gz",
             "filename": f"BRDC00IGS_R_{year}{doy:03d}0000_01D_MN_ign.rnx"},
        ]
        
        out_path = Path(output_dir)
        candidates = []
        
        log_func("   Checking BRDC sources...")
        
        for source in brdc_sources:
            log_func(f"   ⏳ {source['name']}...")
            
            try:
                req = urllib.request.Request(source["url"])
                req.add_header('User-Agent', 'Mozilla/5.0 GNSS-Analysis')
                
                with urllib.request.urlopen(req, timeout=45, context=ctx) as resp:
                    data = resp.read()
                
                if len(data) < 1000:
                    continue
                
                try:
                    decompressed = gzip.decompress(data)
                except:
                    decompressed = data
                
                counts = NavDownloader.count_nav_satellites(decompressed)
                total = sum(counts.values())
                
                if total == 0:
                    continue
                
                summary = NavDownloader.format_sat_summary(counts)
                log_func(f"      ✓ {total} sats: {summary}")
                
                candidates.append({
                    'source': source,
                    'content': decompressed,
                    'counts': counts,
                    'total': total,
                    'summary': summary
                })
                
            except urllib.error.HTTPError as e:
                log_func(f"      ✗ HTTP {e.code}")
            except Exception as e:
                log_func(f"      ✗ {str(e)[:30]}")
        
        if not candidates:
            log_func("   ❌ No BRDC sources available")
            return None
        
        # Pick the best one (most satellites, prefer multi-constellation)
        def score(c):
            num_systems = sum(1 for v in c['counts'].values() if v > 0)
            return (num_systems, c['total'])
        
        best = max(candidates, key=score)
        
        out_file = out_path / best['source']['filename']
        with open(out_file, 'wb') as f:
            f.write(best['content'])
        
        log_func(f"   ✅ Selected: {best['source']['name']}")
        log_func(f"   📊 Ephemeris: {best['summary']}")
        
        return out_file


# ============================================================================
# FILE INPUT WIDGETS
# ============================================================================
header = widgets.HTML(f"""
<h3>📡 GeoVeil CN0 Analysis v{LIB_VERSION}</h3>
<p style="color:#666; font-size:12px;">GNSS Signal Quality Analysis with Interference Detection</p>
""")

# === OBSERVATION FILE ===
obs_section = widgets.HTML("<b>Observation File</b> (required)")

obs_upload = widgets.FileUpload(
    accept='.obs,.rnx,.crx,.24o,.23o,.22o,.21o,.20o,.25o,.gz,.Z,*',
    multiple=False,
    description='Upload OBS',
    button_style='info',
    layout=widgets.Layout(width='200px')
)

obs_path_input = widgets.Text(
    value='',
    placeholder='/path/to/observation.rnx',
    description='OBS Path:',
    style={'description_width': '70px'},
    layout=widgets.Layout(width='450px')
)

# === NAVIGATION FILE ===
nav_section = widgets.HTML("<b>Navigation/Ephemeris</b> (for elevation & skyplots)")

nav_upload = widgets.FileUpload(
    accept='.nav,.rnx,.24n,.24g,.25n,.sp3,.SP3,.gz,.Z,*',
    multiple=False,
    description='Upload NAV',
    button_style='info',
    layout=widgets.Layout(width='200px')
)

nav_path_input = widgets.Text(
    value='',
    placeholder='/path/to/navigation.rnx or .sp3',
    description='NAV Path:',
    style={'description_width': '70px'},
    layout=widgets.Layout(width='450px')
)

auto_download_nav = widgets.Checkbox(
    value=True,
    description='Auto-download BRDC if missing',
    style={'description_width': 'initial'},
    layout=widgets.Layout(width='250px')
)

load_btn = widgets.Button(
    description='📥 Load Files',
    button_style='warning',
    layout=widgets.Layout(width='150px')
)

# === ANALYSIS CONFIG ===
config_section = widgets.HTML("<b>Analysis Configuration</b>")

preset_dropdown = widgets.Dropdown(
    options=[
        ('🔬 Full Analysis', 'full'),
        ('⚡ Quick Summary', 'quick'),
        ('📊 Interference Focus', 'interference'),
        ('🎯 Jamming Detection', 'jamming'),
        ('🛡️ Spoofing Check', 'spoofing'),
    ],
    value='full',
    description='Preset:',
    style={'description_width': 'initial'},
    layout=widgets.Layout(width='280px')
)

preset_help = widgets.HTML(value="""
<div style="font-size:11px; color:#666; margin-top:5px; padding:8px; background:#f8f9fa; border-radius:4px; border-left:3px solid #3182ce;">
<b>Presets:</b> Full (all plots) | Quick (fast) | Interference (>4dB, ITU) | Jamming (>6dB, Stanford) | Spoofing (uniformity)
</div>
""")

elevation_slider = widgets.FloatSlider(
    value=5.0, min=0.0, max=30.0, step=1.0,
    description='Elevation Cutoff (°):',
    style={'description_width': 'initial'},
    layout=widgets.Layout(width='400px')
)

time_bin_slider = widgets.IntSlider(
    value=60, min=10, max=300, step=10,
    description='Time Bin (sec):',
    style={'description_width': 'initial'},
    layout=widgets.Layout(width='400px')
)

system_checks = {
    'G': widgets.Checkbox(value=True, description='GPS', layout=widgets.Layout(width='100px')),
    'R': widgets.Checkbox(value=True, description='GLONASS', layout=widgets.Layout(width='100px')),
    'E': widgets.Checkbox(value=True, description='Galileo', layout=widgets.Layout(width='100px')),
    'C': widgets.Checkbox(value=True, description='BeiDou', layout=widgets.Layout(width='100px')),
}

# === OUTPUT BUTTONS ===
buttons_section = widgets.HTML("<b>📊 Analysis Outputs</b> <i>(click to generate each view)</i>")

btn_summary = widgets.Button(
    description='📊 Summary & Score',
    button_style='primary',
    layout=widgets.Layout(width='160px', height='35px'),
    tooltip='Show text summary and quality radar chart'
)

btn_heatmap = widgets.Button(
    description='🗺️ Heatmaps',
    button_style='info',
    layout=widgets.Layout(width='120px', height='35px'),
    tooltip='Show CN0 heatmaps (Az/El + Time vs Satellite)'
)

btn_snr = widgets.Button(
    description='📈 SNR Graphs',
    button_style='info',
    layout=widgets.Layout(width='130px', height='35px'),
    tooltip='Show CN0 timeseries plots'
)

btn_skyplot = widgets.Button(
    description='🛰️ Skyplot',
    button_style='info',
    layout=widgets.Layout(width='120px', height='35px'),
    tooltip='Show satellite skyplot'
)

btn_anomaly = widgets.Button(
    description='⚠️ Anomalies',
    button_style='warning',
    layout=widgets.Layout(width='130px', height='35px'),
    tooltip='Show anomaly timeline'
)

# === EXPORT BUTTON ===
export_btn = widgets.Button(
    description='📥 Export Full Report',
    button_style='success',
    layout=widgets.Layout(width='180px', height='35px'),
    tooltip='Run complete analysis and download HTML report'
)

clear_btn = widgets.Button(
    description='🗑️ Clear',
    button_style='danger',
    layout=widgets.Layout(width='80px', height='35px')
)

# Progress & Status
progress = widgets.FloatProgress(
    value=0, min=0, max=1.0,
    description='Progress:',
    layout=widgets.Layout(width='400px', visibility='hidden')
)

status = widgets.HTML(value="<b>Status:</b> Ready - load files to begin")

# Output areas
info_out = widgets.Output()
results_out = widgets.Output()


# ============================================================================
# CORE ANALYSIS FUNCTION
# ============================================================================
def run_core_analysis(silent=False):
    """Run the CN0 analysis - returns result or None"""
    global analysis_results
    
    if not HAS_RUST_LIB:
        if not silent:
            with results_out:
                clear_output()
                print("❌ geoveil_cn0 library not installed!")
                print("   Install with: pip install geoveil-cn0")
        return None
    
    if not loaded_data['obs_content']:
        if not silent:
            with results_out:
                clear_output()
                print("❌ No observation file loaded!")
        return None
    
    try:
        # Get preset configuration
        preset = preset_dropdown.value
        preset_cfg = PRESET_CONFIGS.get(preset, PRESET_CONFIGS['full'])
        
        # Get enabled systems
        systems = [s for s, cb in system_checks.items() if cb.value]
        
        # Create config
        config = gcn0.AnalysisConfig(
            min_elevation=elevation_slider.value,
            time_bin_seconds=time_bin_slider.value,
            systems=systems,
            detect_anomalies=True,
            anomaly_sensitivity=preset_cfg['sensitivity'],
            interference_threshold_db=preset_cfg['threshold'],
        )
        
        # Create analyzer
        analyzer = gcn0.CN0Analyzer(config)
        
        # Save content to temp files
        temp_dir = tempfile.gettempdir()
        
        obs_path = os.path.join(temp_dir, loaded_data['obs_filename'])
        with open(obs_path, 'wb') as f:
            f.write(loaded_data['obs_content'])
        
        nav_path = None
        if loaded_data['nav_content']:
            nav_path = os.path.join(temp_dir, loaded_data['nav_filename'])
            with open(nav_path, 'wb') as f:
                f.write(loaded_data['nav_content'])
        
        # Run analysis
        if nav_path:
            result = analyzer.analyze_with_nav(obs_path, nav_path)
        else:
            result = analyzer.analyze_file(obs_path)
        
        analysis_results['data'] = result
        return result
        
    except Exception as e:
        if not silent:
            with results_out:
                clear_output()
                print(f"❌ Analysis error: {e}")
                import traceback
                traceback.print_exc()
        return None


# ============================================================================
# LOCK INTEGRITY CALCULATION
# ============================================================================
def calculate_lock_integrity(result):
    """Calculate lock integrity score from cycle slips and data gaps"""
    total_cycle_slips = 0
    total_data_gaps = 0
    total_satellites = 0
    
    for sys_name in ['GPS', 'GLONASS', 'Galileo', 'BeiDou']:
        stats = result.get_constellation_summary(sys_name)
        if stats:
            total_cycle_slips += int(stats.get('cycle_slips', 0))
            total_data_gaps += int(stats.get('data_gaps', 0))
            total_satellites += int(stats.get('satellite_count', stats.get('satellites_observed', 0)))
    
    # Calculate rates
    duration_hours = max(result.duration_hours, 0.01)
    slips_per_hour = total_cycle_slips / duration_hours
    gaps_per_hour = total_data_gaps / duration_hours
    
    # Lock Integrity Score (0-100, higher is better = fewer losses)
    if total_satellites > 0:
        slips_per_sat_hour = slips_per_hour / total_satellites
        gaps_per_sat_hour = gaps_per_hour / total_satellites
        
        # Score calculation: penalize for slips and gaps
        slip_score = max(0, min(100, 100 - (slips_per_sat_hour * 50)))
        gap_score = max(0, min(100, 100 - (gaps_per_sat_hour * 25)))
        lock_score = (slip_score * 0.6 + gap_score * 0.4)
    else:
        lock_score = 0
        slips_per_sat_hour = 0
    
    return {
        'score': lock_score,
        'total_cycle_slips': total_cycle_slips,
        'total_data_gaps': total_data_gaps,
        'slips_per_hour': slips_per_hour,
        'gaps_per_hour': gaps_per_hour,
        'slips_per_sat_hour': slips_per_sat_hour,
    }


# ============================================================================
# OUTPUT HANDLERS
# ============================================================================
def show_summary(btn):
    """Show text summary and quality score radar"""
    with results_out:
        clear_output()
        print("📊 Generating summary...")
    
    result = analysis_results.get('data')
    if not result:
        result = run_core_analysis()
    
    if not result:
        return
    
    with results_out:
        clear_output()
        
        qs = result.quality_score
        lock_data = calculate_lock_integrity(result)
        
        print("=" * 70)
        print("📊 GEOVEIL CN0 ANALYSIS RESULTS")
        print("=" * 70)
        
        print(f"\n📁 File:")
        print(f"   RINEX Version: {result.rinex_version}")
        print(f"   Duration: {result.duration_hours:.2f} hours ({result.epoch_count} epochs)")
        print(f"   Station: {result.station_name or 'Unknown'}")
        print(f"   Constellations: {', '.join(result.get_systems())}")
        
        print(f"\n🏆 QUALITY SCORE: {qs.overall:.0f}/100 ({qs.rating})")
        print(f"   CN0 Quality:    {qs.cn0_quality:.0f}")
        print(f"   Availability:   {qs.availability:.0f}")
        print(f"   Continuity:     {qs.continuity:.0f}")
        print(f"   Stability:      {qs.stability:.0f}")
        print(f"   Diversity:      {qs.diversity:.0f}")
        print(f"   Lock Integrity: {lock_data['score']:.0f}")
        print(f"   Post-processing: {'✅ Yes' if qs.overall >= 70 else '❌ No'}")
        
        print(f"\n📶 SIGNAL QUALITY:")
        print(f"   Average CN0: {result.mean_cn0:.1f} dB-Hz")
        print(f"   Std Dev: {result.cn0_std_dev:.1f} dB-Hz")
        print(f"   Range: {result.min_cn0:.1f} - {result.max_cn0:.1f} dB-Hz")
        
        print(f"\n🔓 LOCK INTEGRITY:")
        print(f"   Cycle Slips: {lock_data['total_cycle_slips']} ({lock_data['slips_per_hour']:.1f}/hour)")
        print(f"   Data Gaps: {lock_data['total_data_gaps']} ({lock_data['gaps_per_hour']:.1f}/hour)")
        print(f"   Score: {lock_data['score']:.0f}/100")
        
        print(f"\n🛡️ THREAT ASSESSMENT:")
        
        # === IMPROVED THREAT DETECTION ===
        # Don't blindly trust the library's spoofing flag - it has false positives
        # When ephemeris doesn't cover all satellites (especially BeiDou), it triggers falsely
        
        # Jamming: Only if low CN0 + many critical anomalies
        jamming_detected = result.jamming_detected and result.mean_cn0 < 35.0
        
        # Spoofing: Library flag + sanity check
        # Real spoofing: abnormally uniform CN0 across ALL satellites AND elevated CN0
        # Check per-constellation std to avoid false positives from incomplete ephemeris
        constellation_stds = []
        for sys_name in ['GPS', 'GLONASS', 'Galileo', 'BeiDou']:
            stats = result.get_constellation_summary(sys_name)
            if stats:
                try:
                    std_cn0 = float(stats.get('std_cn0', stats.get('cn0_std', 5.0)))
                    constellation_stds.append(std_cn0)
                except:
                    pass
        
        avg_constellation_std = sum(constellation_stds) / len(constellation_stds) if constellation_stds else 5.0
        
        # Real spoofing indicators:
        # 1. Very low CN0 std across constellations (< 2 dB) - signals too uniform
        # 2. AND elevated average CN0 (> 50 dB-Hz) - spoofer typically overpowers
        # 3. AND overall std also low
        spoofing_indicators = []
        if avg_constellation_std < 2.0:
            spoofing_indicators.append("CN0 uniformity suspiciously low")
        if result.mean_cn0 > 50.0:
            spoofing_indicators.append("CN0 elevated (possible high-power signal)")
        if result.cn0_std_dev < 1.0:
            spoofing_indicators.append("Overall signal variance very low")
        
        # Only flag spoofing if multiple indicators present
        spoofing_suspicious = len(spoofing_indicators) >= 2
        
        # If library says spoofing but our checks don't agree, it's likely false positive from ephemeris
        if result.spoofing_detected and not spoofing_suspicious:
            print(f"   Jamming:      {'🚨 DETECTED' if jamming_detected else '✅ None'}")
            print(f"   Spoofing:     ⚠️ Flag raised (likely false positive - incomplete ephemeris)")
            print(f"   Interference: {'⚠️ Detected' if result.interference_detected else '✅ None'}")
        else:
            print(f"   Jamming:      {'🚨 DETECTED' if jamming_detected else '✅ None'}")
            print(f"   Spoofing:     {'🚨 DETECTED' if spoofing_suspicious else '✅ None'}")
            print(f"   Interference: {'⚠️ Detected' if result.interference_detected else '✅ None'}")
        
        if spoofing_indicators and spoofing_suspicious:
            print(f"   └─ Indicators: {', '.join(spoofing_indicators)}")
        
        # === GENERATE PROPER SUMMARY BASED ON DISPLAYED SCORE ===
        # Don't use result.summary - it's based on internal score with different weights
        overall_score = qs.overall
        
        if overall_score >= 90:
            quality_text = "Excellent GNSS signal quality"
        elif overall_score >= 80:
            quality_text = "Good GNSS signal quality"
        elif overall_score >= 70:
            quality_text = "Fair GNSS signal quality"
        elif overall_score >= 60:
            quality_text = "Degraded GNSS signal quality"
        else:
            quality_text = "Poor GNSS signal quality"
        
        # Add warnings if needed
        warnings = []
        if jamming_detected:
            warnings.append("jamming detected")
        if spoofing_suspicious:
            warnings.append("spoofing indicators present")
        if result.interference_detected:
            warnings.append("interference events detected")
        if lock_data['score'] < 50:
            warnings.append("significant lock loss issues")
        
        if warnings:
            summary_text = f"{quality_text} - WARNING: {', '.join(warnings)}"
        else:
            summary_text = quality_text
        
        print(f"\n📝 {summary_text}")
        
        # === ANALYSIS CONCLUSION ===
        print(f"\n" + "=" * 70)
        print(f"📋 CONCLUSION:")
        print(f"=" * 70)
        
        conclusions = []
        
        # Overall assessment
        if overall_score >= 90:
            conclusions.append("✅ Data quality is EXCELLENT - suitable for high-precision applications (PPP/PPK)")
        elif overall_score >= 80:
            conclusions.append("✅ Data quality is GOOD - suitable for standard GNSS applications")
        elif overall_score >= 70:
            conclusions.append("⚠️ Data quality is FAIR - usable but may have reduced accuracy")
        elif overall_score >= 60:
            conclusions.append("⚠️ Data quality is DEGRADED - review anomalies before use")
        else:
            conclusions.append("❌ Data quality is POOR - significant issues detected")
        
        # Signal strength assessment
        if result.mean_cn0 >= 45:
            conclusions.append(f"✅ Signal strength EXCELLENT ({result.mean_cn0:.1f} dB-Hz average)")
        elif result.mean_cn0 >= 40:
            conclusions.append(f"✅ Signal strength GOOD ({result.mean_cn0:.1f} dB-Hz average)")
        elif result.mean_cn0 >= 35:
            conclusions.append(f"⚠️ Signal strength MODERATE ({result.mean_cn0:.1f} dB-Hz average)")
        else:
            conclusions.append(f"❌ Signal strength LOW ({result.mean_cn0:.1f} dB-Hz) - possible interference")
        
        # Constellation coverage
        total_sats = sum(int(result.get_constellation_summary(s).get('satellite_count', 0)) 
                        for s in ['GPS', 'GLONASS', 'Galileo', 'BeiDou'] 
                        if result.get_constellation_summary(s))
        if total_sats >= 40:
            conclusions.append(f"✅ Excellent multi-GNSS coverage ({total_sats} satellites)")
        elif total_sats >= 25:
            conclusions.append(f"✅ Good satellite coverage ({total_sats} satellites)")
        else:
            conclusions.append(f"⚠️ Limited satellite coverage ({total_sats} satellites)")
        
        # Lock integrity
        if lock_data['score'] >= 80:
            conclusions.append("✅ Signal continuity is excellent (minimal lock losses)")
        elif lock_data['score'] >= 60:
            conclusions.append("✅ Signal continuity is acceptable")
        else:
            conclusions.append(f"⚠️ Signal continuity issues detected ({lock_data['total_data_gaps']} data gaps)")
        
        # Threat summary
        if not (jamming_detected or spoofing_suspicious or result.interference_detected):
            conclusions.append("✅ No significant threats detected")
        else:
            if jamming_detected:
                conclusions.append("🚨 JAMMING DETECTED - data may be compromised")
            if spoofing_suspicious:
                conclusions.append("🚨 SPOOFING INDICATORS - verify data integrity")
            if result.interference_detected:
                conclusions.append("⚠️ Interference events detected - review anomaly timeline")
        
        # Post-processing recommendation
        if overall_score >= 70 and result.mean_cn0 >= 35 and lock_data['score'] >= 50:
            conclusions.append("✅ Data suitable for post-processing (PPP/RTK)")
        else:
            conclusions.append("⚠️ Review issues before post-processing")
        
        for c in conclusions:
            print(f"   {c}")
        
        print(f"\n" + "=" * 70)
        
        # Constellation summary
        print(f"\n🛰️ CONSTELLATION SUMMARY:")
        for sys_name in ['GPS', 'GLONASS', 'Galileo', 'BeiDou']:
            stats = result.get_constellation_summary(sys_name)
            if stats:
                sat_count = stats.get('satellite_count', stats.get('satellites_observed', 0))
                expected = stats.get('satellites_expected', sat_count)
                mean_cn0 = float(stats.get('mean_cn0', stats.get('cn0_mean', 0)))
                std_cn0 = float(stats.get('std_cn0', stats.get('cn0_std', 0)))
                slips = stats.get('cycle_slips', 0)
                gaps = stats.get('data_gaps', 0)
                
                print(f"\n   {sys_name}:")
                print(f"      Satellites: {sat_count}/{expected}")
                print(f"      CN0: {mean_cn0:.1f} ± {std_cn0:.1f} dB-Hz")
                print(f"      Cycle Slips: {slips}, Data Gaps: {gaps}")
        
        print("\n" + "=" * 70)
        
        # Quality Radar Chart
        print("\n📈 Quality Score Radar:")
        
        categories = ['Availability', 'CN0 Quality', 'Stability', 'Diversity', 'Continuity', 'Lock Integrity']
        values = [qs.availability, qs.cn0_quality, qs.stability, qs.diversity, qs.continuity, lock_data['score']]
        values.append(values[0])
        
        fig = go.Figure()
        fig.add_trace(go.Scatterpolar(
            r=values,
            theta=categories + [categories[0]],
            fill='toself',
            fillcolor='rgba(99, 110, 250, 0.3)',
            line=dict(color='rgb(99, 110, 250)', width=2),
            name='Quality'
        ))
        
        fig.update_layout(
            polar=dict(radialaxis=dict(visible=True, range=[0, 100])),
            showlegend=False,
            title=f"Quality Score: {qs.overall:.0f}/100 ({qs.rating})",
            height=550,
            width=650
        )
        
        fig.show()
        
        # Store lock integrity for export
        analysis_results['lock_integrity'] = lock_data


def show_heatmap(btn):
    """Show BOTH CN0 heatmaps: Time vs Satellite and Az/El"""
    with results_out:
        clear_output()
        print("🗺️ Generating heatmaps...")
    
    result = analysis_results.get('data')
    if not result:
        result = run_core_analysis()
    
    if not result:
        return
    
    with results_out:
        clear_output()
        
        # =====================================================================
        # HEATMAP 1: Time vs Satellite PRN
        # =====================================================================
        print("🔥 C/N₀ Heatmap - Time vs Satellite")
        
        try:
            result_json = json.loads(result.to_json())
            sat_timeseries = result_json.get('timeseries', {}).get('satellite_timeseries', {})
            
            if sat_timeseries and len(sat_timeseries) > 0:
                print(f"   Found {len(sat_timeseries)} satellites in timeseries")
                
                # Get all timestamps
                all_times = set()
                for sat_id, sat_data in sat_timeseries.items():
                    if isinstance(sat_data, dict):
                        series = sat_data.get('cn0_series', sat_data.get('series', []))
                        if isinstance(series, list):
                            for point in series:
                                if isinstance(point, dict):
                                    all_times.add(point.get('timestamp', point.get('time', '')))
                
                all_times = sorted([t for t in all_times if t])
                
                if all_times:
                    # Subsample if too many points
                    max_time_points = 500
                    if len(all_times) > max_time_points:
                        step = len(all_times) // max_time_points
                        all_times = all_times[::step]
                    
                    # Sort satellites
                    def sat_sort_key(s):
                        if len(s) >= 2:
                            sys = s[0]
                            try:
                                prn = int(s[1:])
                            except:
                                prn = 0
                            sys_order = {'G': 0, 'R': 1, 'E': 2, 'C': 3, 'J': 4, 'I': 5}
                            return (sys_order.get(sys, 9), prn)
                        return (9, 0)
                    
                    all_satellites = sorted(sat_timeseries.keys(), key=sat_sort_key, reverse=True)
                    
                    # Build z matrix
                    z_matrix = []
                    sat_labels = []
                    
                    for sat in all_satellites:
                        sat_data = sat_timeseries.get(sat, {})
                        if isinstance(sat_data, dict):
                            cn0_series = sat_data.get('cn0_series', sat_data.get('series', []))
                        else:
                            continue
                        
                        cn0_by_time = {}
                        if isinstance(cn0_series, list):
                            for p in cn0_series:
                                if isinstance(p, dict):
                                    t = p.get('timestamp', p.get('time', ''))
                                    v = p.get('value', p.get('cn0', None))
                                    if t and v is not None:
                                        cn0_by_time[t] = v
                        
                        row = [cn0_by_time.get(t, None) for t in all_times]
                        valid_count = sum(1 for v in row if v is not None)
                        if valid_count > len(all_times) * 0.05:
                            z_matrix.append(row)
                            sat_labels.append(sat)
                    
                    if z_matrix:
                        time_labels = pd.to_datetime(all_times)
                        
                        fig1 = go.Figure(go.Heatmap(
                            z=z_matrix,
                            x=time_labels,
                            y=sat_labels,
                            colorscale='Viridis',
                            colorbar=dict(title='C/N₀<br>(dB-Hz)'),
                            hoverongaps=False,
                            hovertemplate='Satellite: %{y}<br>Time: %{x}<br>CN0: %{z:.1f} dB-Hz<extra></extra>',
                            zmin=25, zmax=55
                        ))
                        
                        fig1.update_layout(
                            title='🔥 C/N₀ Heatmap - Time vs Satellite',
                            xaxis_title='Time (UTC)',
                            yaxis_title='Satellite PRN',
                            width=1100,
                            height=max(400, len(sat_labels) * 20 + 150),
                            xaxis=dict(type='date'),
                        )
                        
                        fig1.show()
                    else:
                        print("⚠️ No valid satellite data for heatmap")
                else:
                    print("⚠️ No timestamps found")
            else:
                print("⚠️ No satellite_timeseries data available")
        except Exception as e:
            print(f"⚠️ Could not create Time vs Satellite heatmap: {e}")
        
        # =====================================================================
        # HEATMAP 2: CN0 by Azimuth/Elevation
        # =====================================================================
        print("\n🗺️ CN0 Heatmap by Azimuth/Elevation")
        
        skyplot_data = result.get_skyplot_data()
        
        if not skyplot_data:
            print("⚠️ No azimuth/elevation data (need navigation file)")
            return
        
        az_bins = list(range(0, 361, 15))
        el_bins = list(range(0, 91, 5))
        
        cn0_sum = [[0.0 for _ in range(len(az_bins)-1)] for _ in range(len(el_bins)-1)]
        cn0_count = [[0 for _ in range(len(az_bins)-1)] for _ in range(len(el_bins)-1)]
        
        for sat_trace in skyplot_data:
            azimuths = [float(x) for x in sat_trace.get('azimuths', '').split(',') if x]
            elevations = [float(x) for x in sat_trace.get('elevations', '').split(',') if x]
            cn0_values = [float(x) for x in sat_trace.get('cn0_values', '').split(',') if x]
            
            for az, el, cn0 in zip(azimuths, elevations, cn0_values):
                az_idx = min(int(az / 15), len(az_bins) - 2)
                el_idx = min(int(el / 5), len(el_bins) - 2)
                cn0_sum[el_idx][az_idx] += cn0
                cn0_count[el_idx][az_idx] += 1
        
        cn0_grid = []
        for el_idx in range(len(el_bins) - 1):
            row = []
            for az_idx in range(len(az_bins) - 1):
                if cn0_count[el_idx][az_idx] > 0:
                    row.append(cn0_sum[el_idx][az_idx] / cn0_count[el_idx][az_idx])
                else:
                    row.append(None)
            cn0_grid.append(row)
        
        if all(all(v is None for v in row) for row in cn0_grid):
            print("⚠️ No heatmap data available")
            return
        
        fig2 = go.Figure(go.Heatmap(
            z=cn0_grid,
            x=[f"{az_bins[i]}-{az_bins[i+1]}" for i in range(len(az_bins)-1)],
            y=[f"{el_bins[i]}-{el_bins[i+1]}" for i in range(len(el_bins)-1)],
            colorscale='Viridis',
            colorbar=dict(title='CN0 (dB-Hz)'),
            hoverongaps=False,
            zmin=30, zmax=55
        ))
        
        fig2.update_layout(
            title='CN0 Heatmap by Azimuth/Elevation',
            xaxis_title='Azimuth (°)',
            yaxis_title='Elevation (°)',
            width=850,
            height=500
        )
        
        fig2.show()


def show_snr_graphs(btn):
    """Show CN0 timeseries - Overall + Per-constellation"""
    with results_out:
        clear_output()
        print("📈 Generating SNR timeseries...")
    
    result = analysis_results.get('data')
    if not result:
        result = run_core_analysis()
    
    if not result:
        return
    
    with results_out:
        clear_output()
        
        timestamps = result.get_timestamps()
        
        if not timestamps:
            print("⚠️ No timeseries data available")
            return
        
        ts = pd.to_datetime(timestamps)
        mean_cn0 = result.get_mean_cn0_series()
        sat_counts = result.get_satellite_count_series()
        
        if len(mean_cn0) == 0:
            print("⚠️ No CN0 timeseries data")
            return
        
        # Overall Mean CN0 Timeseries
        print("📊 Overall CN0 Timeseries:")
        
        fig = make_subplots(
            rows=2, cols=1,
            shared_xaxes=True,
            vertical_spacing=0.08,
            row_heights=[0.7, 0.3],
            subplot_titles=('CN0 Timeseries', 'Satellite Count')
        )
        
        fig.add_trace(go.Scatter(
            x=ts, y=mean_cn0,
            name='Overall Mean',
            line=dict(color='black', width=2.5)
        ), row=1, col=1)
        
        fig.add_trace(go.Bar(
            x=ts, y=sat_counts,
            marker_color='#6b7280',
            showlegend=False
        ), row=2, col=1)
        
        fig.add_hline(y=35, line_dash='dash', line_color='orange', 
                      annotation_text='Degraded (35)', row=1, col=1)
        
        fig.update_layout(height=500, width=1100, title='CN0 Timeseries',
                          legend=dict(orientation='h', y=1.12))
        fig.update_yaxes(title_text='CN0 (dB-Hz)', row=1, col=1)
        fig.update_yaxes(title_text='Satellites', row=2, col=1)
        fig.update_xaxes(title_text='Time (UTC)', row=2, col=1)
        
        fig.show()
        
        # Per-Constellation Satellite CN0 Timeseries
        sat_colors = [
            '#636EFA', '#EF553B', '#00CC96', '#AB63FA', '#FFA15A',
            '#19D3F3', '#FF6692', '#B6E880', '#FF97FF', '#FECB52',
            '#1F77B4', '#FF7F0E', '#2CA02C', '#D62728', '#9467BD',
        ]
        
        sys_to_const = {'G': 'GPS', 'R': 'GLONASS', 'E': 'Galileo', 'C': 'BeiDou'}
        
        try:
            result_json = json.loads(result.to_json())
            sat_timeseries = result_json.get('timeseries', {}).get('satellite_timeseries', {})
            
            if not sat_timeseries:
                print("\n⚠️ No per-satellite data available")
                return
            
            const_satellites = {'GPS': [], 'GLONASS': [], 'Galileo': [], 'BeiDou': []}
            
            for sat_id, sat_data in sat_timeseries.items():
                if len(sat_id) < 2:
                    continue
                
                system = sat_id[0]
                const_name = sys_to_const.get(system)
                if not const_name:
                    continue
                
                if isinstance(sat_data, dict):
                    cn0_series = sat_data.get('cn0_series', sat_data.get('series', []))
                else:
                    continue
                
                sat_timestamps = []
                sat_cn0_values = []
                
                if isinstance(cn0_series, list):
                    for p in cn0_series:
                        if isinstance(p, dict):
                            t = p.get('timestamp', p.get('time', ''))
                            v = p.get('value', p.get('cn0', None))
                            if t and v is not None:
                                sat_timestamps.append(t)
                                sat_cn0_values.append(v)
                
                if len(sat_cn0_values) > 0:
                    const_satellites[const_name].append({
                        'sat_id': sat_id,
                        'timestamps': pd.to_datetime(sat_timestamps),
                        'cn0_values': sat_cn0_values
                    })
            
            for const_name, satellites in const_satellites.items():
                if not satellites:
                    continue
                
                satellites.sort(key=lambda s: int(s['sat_id'][1:]) if s['sat_id'][1:].isdigit() else 0)
                
                print(f"\n📡 {const_name} C/N₀ Timeseries ({len(satellites)} satellites):")
                
                fig_const = go.Figure()
                
                for i, sat_data in enumerate(satellites):
                    color = sat_colors[i % len(sat_colors)]
                    fig_const.add_trace(go.Scatter(
                        x=sat_data['timestamps'],
                        y=sat_data['cn0_values'],
                        name=sat_data['sat_id'],
                        mode='lines',
                        line=dict(width=1.5, color=color),
                    ))
                
                fig_const.add_hline(y=35, line_dash='dash', line_color='orange')
                fig_const.add_hline(y=25, line_dash='dash', line_color='red')
                
                fig_const.update_layout(
                    title=f'📡 {const_name} - C/N₀ by Satellite',
                    xaxis_title='Time (UTC)',
                    yaxis_title='C/N₀ (dB-Hz)',
                    height=450, width=1100,
                    yaxis=dict(range=[20, 60]),
                    legend=dict(orientation='h', y=1.02, font=dict(size=10)),
                    hovermode='x unified'
                )
                
                fig_const.show()
                
        except Exception as e:
            print(f"\n⚠️ Could not create per-constellation graphs: {e}")


def show_skyplot(btn):
    """Show satellite skyplot"""
    with results_out:
        clear_output()
        print("🛰️ Generating skyplot...")
    
    result = analysis_results.get('data')
    if not result:
        result = run_core_analysis()
    
    if not result:
        return
    
    with results_out:
        clear_output()
        
        skyplot_data = result.get_skyplot_data()
        coverage = result.skyplot_coverage
        
        if not skyplot_data:
            print("⚠️ No skyplot data (need navigation file)")
            return
        
        fig = go.Figure()
        
        const_colors = {'GPS': '#3b82f6', 'GLONASS': '#ef4444', 'Galileo': '#22c55e', 'BeiDou': '#f59e0b'}
        const_data = {}
        
        for sat_trace in skyplot_data:
            system = sat_trace.get('system', 'Other')
            const_name = {'G': 'GPS', 'R': 'GLONASS', 'E': 'Galileo', 'C': 'BeiDou'}.get(system, system)
            
            if const_name not in const_data:
                const_data[const_name] = {'r': [], 'theta': [], 'cn0': [], 'text': []}
            
            azimuths = [float(x) for x in sat_trace.get('azimuths', '').split(',') if x]
            elevations = [float(x) for x in sat_trace.get('elevations', '').split(',') if x]
            cn0_values = [float(x) for x in sat_trace.get('cn0_values', '').split(',') if x]
            sat_id = sat_trace.get('satellite', '')
            
            for az, el, cn0 in zip(azimuths, elevations, cn0_values):
                r = 90 - el
                const_data[const_name]['r'].append(r)
                const_data[const_name]['theta'].append(az)
                const_data[const_name]['cn0'].append(cn0)
                const_data[const_name]['text'].append(f"{sat_id}: {cn0:.1f} dB-Hz")
        
        for i, (const_name, data) in enumerate(const_data.items()):
            if data['r']:
                fig.add_trace(go.Scatterpolar(
                    r=data['r'],
                    theta=data['theta'],
                    mode='markers',
                    name=const_name,
                    marker=dict(
                        size=8,
                        color=data['cn0'],
                        colorscale='Viridis',
                        cmin=25, cmax=55,
                        colorbar=dict(title='CN0 (dB-Hz)') if i == 0 else None,
                        showscale=(i == 0)
                    ),
                    text=data['text'],
                    hoverinfo='text'
                ))
        
        fig.update_layout(
            title=f'CN0 Skyplot (Coverage: {coverage:.1f}%)',
            width=650, height=650,
            polar=dict(
                radialaxis=dict(range=[0, 90], tickvals=[0, 30, 60, 90], ticktext=['90°', '60°', '30°', '0°']),
                angularaxis=dict(tickvals=[0, 45, 90, 135, 180, 225, 270, 315],
                                 ticktext=['N', 'NE', 'E', 'SE', 'S', 'SW', 'W', 'NW'],
                                 direction='clockwise', rotation=90)
            )
        )
        
        fig.show()


def show_anomalies(btn):
    """Show anomaly timeline with robust timestamp parsing"""
    with results_out:
        clear_output()
        print("⚠️ Generating anomaly timeline...")
    
    result = analysis_results.get('data')
    if not result:
        result = run_core_analysis()
    
    if not result:
        return
    
    with results_out:
        clear_output()
        
        anomalies = result.get_anomalies()
        
        if not anomalies:
            print("✅ No anomalies detected!")
            fig = go.Figure()
            fig.add_annotation(text='✅ No anomalies detected', xref='paper', yref='paper',
                               x=0.5, y=0.5, showarrow=False, font=dict(size=24, color='green'))
            fig.update_layout(title='Anomaly Timeline', height=400)
            fig.show()
            return
        
        print(f"⚠️ Found {len(anomalies)} anomalies")
        
        # Parse anomalies using pd.to_datetime (robust!)
        parsed = []
        for a in anomalies:
            try:
                ts_str = a.get('start_time') or a.get('timestamp') or ''
                if ts_str:
                    ts = pd.to_datetime(ts_str, errors='coerce')
                    if pd.notna(ts):
                        parsed.append({
                            'time': ts,
                            'severity': str(a.get('severity', 'low')).lower(),
                            'cn0_drop': float(a.get('cn0_drop', a.get('cn0_drop_db', 0)) or 0),
                            'type': a.get('anomaly_type', a.get('type', 'Unknown')),
                            'description': a.get('description', ''),
                            'confidence': float(a.get('confidence', 0.5) or 0.5),
                        })
            except:
                continue
        
        if not parsed:
            print("⚠️ Could not parse anomaly timestamps")
            return
        
        fig = go.Figure()
        
        severity_colors = {'critical': '#ef4444', 'high': '#f97316', 'medium': '#eab308', 'low': '#22c55e'}
        
        for severity in ['critical', 'high', 'medium', 'low']:
            sev_data = [d for d in parsed if d['severity'] == severity]
            if sev_data:
                fig.add_trace(go.Scatter(
                    x=[d['time'] for d in sev_data],
                    y=[d['cn0_drop'] for d in sev_data],
                    mode='markers',
                    marker=dict(size=[8 + d['confidence'] * 10 for d in sev_data],
                                color=severity_colors.get(severity, '#888'), opacity=0.7),
                    name=f"{severity.capitalize()} ({len(sev_data)})",
                    text=[f"{d['type']}<br>{d['description']}" for d in sev_data],
                    hovertemplate="Time: %{x}<br>CN0 Drop: %{y:.1f} dB<br>%{text}<extra></extra>"
                ))
        
        # Add preset-specific threshold line
        preset = preset_dropdown.value
        if preset == 'jamming':
            fig.add_hline(y=6.0, line_dash="dash", line_color="red", annotation_text="Jamming (6 dB)")
        elif preset == 'interference':
            fig.add_hline(y=4.0, line_dash="dash", line_color="orange", annotation_text="Interference (4 dB)")
        
        fig.update_layout(
            title=f'Anomaly Timeline ({len(parsed)} events)',
            xaxis_title='Time (UTC)',
            xaxis=dict(type='date'),
            yaxis_title='CN0 Drop (dB)',
            height=450,
            legend=dict(orientation='h', y=1.1)
        )
        
        fig.show()


def export_report(btn):
    """Run full analysis and offer download"""
    with results_out:
        clear_output()
        print("📥 Generating full report...")
        progress.layout.visibility = 'visible'
        progress.value = 0.1
    
    result = run_core_analysis(silent=True)
    
    if not result:
        with results_out:
            clear_output()
            print("❌ Could not generate report - check files are loaded")
        progress.layout.visibility = 'hidden'
        return
    
    progress.value = 0.5
    
    try:
        import base64
        
        qs = result.quality_score
        anomalies = result.get_anomalies()
        lock_data = calculate_lock_integrity(result)
        
        # Generate HTML report
        html = f"""<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <title>GeoVeil CN0 Report - {result.filename}</title>
    <style>
        body {{ font-family: Arial, sans-serif; margin: 40px; background: #f5f5f5; }}
        .container {{ max-width: 1200px; margin: 0 auto; background: white; padding: 30px; border-radius: 10px; box-shadow: 0 2px 10px rgba(0,0,0,0.1); }}
        h1 {{ color: #1a365d; border-bottom: 3px solid #3182ce; padding-bottom: 10px; }}
        h2 {{ color: #2c5282; margin-top: 30px; }}
        .score-box {{ background: linear-gradient(135deg, #667eea 0%, #764ba2 100%); color: white; padding: 20px; border-radius: 10px; text-align: center; margin: 20px 0; }}
        .score-value {{ font-size: 48px; font-weight: bold; }}
        .score-rating {{ font-size: 24px; opacity: 0.9; }}
        .metric-grid {{ display: grid; grid-template-columns: repeat(auto-fit, minmax(180px, 1fr)); gap: 15px; margin: 20px 0; }}
        .metric-card {{ background: #f7fafc; padding: 15px; border-radius: 8px; border-left: 4px solid #3182ce; }}
        .metric-value {{ font-size: 24px; font-weight: bold; color: #2d3748; }}
        .metric-label {{ color: #718096; font-size: 14px; }}
        .status-ok {{ color: #38a169; }}
        .status-warning {{ color: #d69e2e; }}
        .status-danger {{ color: #e53e3e; }}
        .anomaly {{ background: #fff5f5; border-left: 4px solid #e53e3e; padding: 10px; margin: 5px 0; border-radius: 4px; }}
        .footer {{ margin-top: 40px; padding-top: 20px; border-top: 1px solid #e2e8f0; color: #718096; font-size: 12px; }}
    </style>
</head>
<body>
<div class="container">
    <h1>📡 GeoVeil CN0 Analysis Report</h1>
    
    <h2>📁 File Information</h2>
    <div class="metric-grid">
        <div class="metric-card"><div class="metric-label">Filename</div><div class="metric-value" style="font-size:14px;">{result.filename}</div></div>
        <div class="metric-card"><div class="metric-label">RINEX Version</div><div class="metric-value">{result.rinex_version}</div></div>
        <div class="metric-card"><div class="metric-label">Duration</div><div class="metric-value">{result.duration_hours:.2f} h</div></div>
        <div class="metric-card"><div class="metric-label">Epochs</div><div class="metric-value">{result.epoch_count:,}</div></div>
    </div>
    
    <h2>🏆 Quality Score</h2>
    <div class="score-box">
        <div class="score-value">{qs.overall:.0f}/100</div>
        <div class="score-rating">{qs.rating}</div>
    </div>
    <div class="metric-grid">
        <div class="metric-card"><div class="metric-label">CN0 Quality</div><div class="metric-value">{qs.cn0_quality:.0f}</div></div>
        <div class="metric-card"><div class="metric-label">Availability</div><div class="metric-value">{qs.availability:.0f}</div></div>
        <div class="metric-card"><div class="metric-label">Continuity</div><div class="metric-value">{qs.continuity:.0f}</div></div>
        <div class="metric-card"><div class="metric-label">Stability</div><div class="metric-value">{qs.stability:.0f}</div></div>
        <div class="metric-card"><div class="metric-label">Diversity</div><div class="metric-value">{qs.diversity:.0f}</div></div>
        <div class="metric-card"><div class="metric-label">Lock Integrity</div><div class="metric-value">{lock_data['score']:.0f}</div></div>
    </div>
    
    <h2>📶 Signal Quality</h2>
    <div class="metric-grid">
        <div class="metric-card"><div class="metric-label">Average CN0</div><div class="metric-value">{result.mean_cn0:.1f} dB-Hz</div></div>
        <div class="metric-card"><div class="metric-label">Std Deviation</div><div class="metric-value">{result.cn0_std_dev:.1f} dB-Hz</div></div>
        <div class="metric-card"><div class="metric-label">Range</div><div class="metric-value">{result.min_cn0:.1f} - {result.max_cn0:.1f}</div></div>
    </div>
    
    <h2>🔓 Lock Integrity</h2>
    <div class="metric-grid">
        <div class="metric-card"><div class="metric-label">Cycle Slips</div><div class="metric-value">{lock_data['total_cycle_slips']}</div><div style="font-size:11px;">{lock_data['slips_per_hour']:.1f}/hour</div></div>
        <div class="metric-card"><div class="metric-label">Data Gaps</div><div class="metric-value">{lock_data['total_data_gaps']}</div></div>
        <div class="metric-card"><div class="metric-label">Lock Score</div><div class="metric-value {'status-ok' if lock_data['score'] >= 70 else 'status-warning' if lock_data['score'] >= 50 else 'status-danger'}">{lock_data['score']:.0f}/100</div></div>
    </div>
    
    <h2>🛡️ Threat Assessment</h2>
    <div class="metric-grid">
        <div class="metric-card"><div class="metric-label">Jamming</div><div class="metric-value {'status-danger' if result.jamming_detected else 'status-ok'}">{'🚨 DETECTED' if result.jamming_detected else '✅ None'}</div></div>
        <div class="metric-card"><div class="metric-label">Spoofing</div><div class="metric-value {'status-danger' if result.spoofing_detected else 'status-ok'}">{'🚨 DETECTED' if result.spoofing_detected else '✅ None'}</div></div>
        <div class="metric-card"><div class="metric-label">Interference</div><div class="metric-value {'status-warning' if result.interference_detected else 'status-ok'}">{'⚠️ Detected' if result.interference_detected else '✅ None'}</div></div>
    </div>
    
    <h2>⚠️ Anomalies ({len(anomalies) if anomalies else 0})</h2>
    {'<p>No anomalies detected.</p>' if not anomalies else ''.join([f"<div class='anomaly'><b>{a.get('anomaly_type', 'Unknown')}</b> ({a.get('severity', 'low')}) - {a.get('start_time', '')}</div>" for a in anomalies[:20]])}
    
    <h2>📝 Summary</h2>
    <p>{result.summary}</p>
    
    <div class="footer">
        <p>Generated by GeoVeil CN0 v{LIB_VERSION} | {datetime.now().strftime('%Y-%m-%d %H:%M:%S')}</p>
    </div>
</div>
</body>
</html>"""
        
        progress.value = 1.0
        
        filename = f"cn0_report_{datetime.now().strftime('%Y%m%d_%H%M%S')}.html"
        b64 = base64.b64encode(html.encode()).decode()
        
        with results_out:
            clear_output()
            print(f"✅ Report generated: {filename}")
            print(f"   Quality Score: {qs.overall:.0f}/100 ({qs.rating})")
            print(f"   Anomalies: {len(anomalies) if anomalies else 0}")
            print()
            
            download_link = f'<a download="{filename}" href="data:text/html;base64,{b64}" style="font-size:18px; padding:10px 20px; background:#22c55e; color:white; text-decoration:none; border-radius:5px;">📥 Download Report ({len(html)//1024} KB)</a>'
            display(HTML(download_link))
        
        progress.layout.visibility = 'hidden'
        
    except Exception as e:
        with results_out:
            clear_output()
            print(f"❌ Export error: {e}")
        progress.layout.visibility = 'hidden'


def clear_all(btn):
    """Clear all outputs"""
    with info_out:
        clear_output()
    with results_out:
        clear_output()
    analysis_results['data'] = None
    analysis_results['figures'] = {}
    status.value = "<b>Status:</b> Cleared"


def load_files(btn):
    """Load files from paths OR upload widgets"""
    with info_out:
        clear_output()
        print("📥 Loading files...")
        
        obs_loaded = False
        nav_loaded = False
        
        # OBSERVATION FILE
        if obs_path_input.value.strip():
            path = obs_path_input.value.strip()
            print(f"\n📂 Loading OBS from path: {path}")
            
            if os.path.exists(path):
                try:
                    with open(path, 'rb') as f:
                        content = f.read()
                    
                    if path.endswith('.gz'):
                        content = gzip.decompress(content)
                    
                    loaded_data['obs_content'] = content
                    loaded_data['obs_filename'] = os.path.basename(path)
                    loaded_data['obs_path'] = path
                    obs_loaded = True
                    print(f"   ✅ Loaded: {loaded_data['obs_filename']} ({len(content)/1024:.1f} KB)")
                except Exception as e:
                    print(f"   ❌ Error: {e}")
            else:
                print(f"   ❌ File not found: {path}")
        
        elif obs_upload.value:
            file_info = list(obs_upload.value.values())[0]
            content = file_info['content']
            filename = file_info['metadata']['name']
            
            print(f"\n📤 Loading uploaded OBS: {filename}")
            
            if filename.endswith('.gz'):
                content = gzip.decompress(content)
                filename = filename[:-3]
            
            loaded_data['obs_content'] = content
            loaded_data['obs_filename'] = filename
            obs_loaded = True
            print(f"   ✅ Loaded: {filename} ({len(content)/1024:.1f} KB)")
        
        # NAVIGATION FILE
        if nav_path_input.value.strip():
            path = nav_path_input.value.strip()
            print(f"\n📂 Loading NAV from path: {path}")
            
            if os.path.exists(path):
                try:
                    with open(path, 'rb') as f:
                        content = f.read()
                    
                    if path.endswith('.gz'):
                        content = gzip.decompress(content)
                    
                    loaded_data['nav_content'] = content
                    loaded_data['nav_filename'] = os.path.basename(path)
                    loaded_data['nav_path'] = path
                    nav_loaded = True
                    print(f"   ✅ Loaded: {loaded_data['nav_filename']} ({len(content)/1024:.1f} KB)")
                except Exception as e:
                    print(f"   ❌ Error: {e}")
            else:
                print(f"   ❌ File not found: {path}")
        
        elif nav_upload.value:
            file_info = list(nav_upload.value.values())[0]
            content = file_info['content']
            filename = file_info['metadata']['name']
            
            print(f"\n📤 Loading uploaded NAV: {filename}")
            
            if filename.endswith('.gz'):
                content = gzip.decompress(content)
                filename = filename[:-3]
            
            loaded_data['nav_content'] = content
            loaded_data['nav_filename'] = filename
            nav_loaded = True
            print(f"   ✅ Loaded: {filename} ({len(content)/1024:.1f} KB)")
        
        # AUTO-DOWNLOAD NAV
        if not nav_loaded and obs_loaded and auto_download_nav.value:
            print("\n🌐 Attempting to auto-download navigation...")
            
            year, doy = NavDownloader.parse_rinex_header(loaded_data['obs_content'])
            
            if not year or not doy:
                year, doy = NavDownloader.parse_rinex_date(loaded_data['obs_filename'])
            
            if year and doy:
                print(f"   Detected date: {year} DOY {doy}")
                
                nav_path = NavDownloader.download(year, doy, tempfile.gettempdir(), log_func=print)
                
                if nav_path and nav_path.exists():
                    with open(nav_path, 'rb') as f:
                        loaded_data['nav_content'] = f.read()
                    loaded_data['nav_filename'] = nav_path.name
                    loaded_data['nav_path'] = str(nav_path)
                    nav_loaded = True
            else:
                print("   ⚠️ Could not determine date from filename or header")
        
        # SUMMARY
        print("\n" + "=" * 50)
        if obs_loaded:
            print(f"✅ OBS: {loaded_data['obs_filename']}")
        else:
            print("❌ OBS: Not loaded")
        
        if nav_loaded:
            print(f"✅ NAV: {loaded_data['nav_filename']}")
        else:
            print("⚠️ NAV: Not loaded (skyplots will be limited)")
        
        if obs_loaded:
            status.value = f"<b>Status:</b> ✅ Files loaded - click output buttons to analyze"
            analysis_results['data'] = None
        else:
            status.value = "<b>Status:</b> ❌ No observation file loaded"


# ============================================================================
# CONNECT HANDLERS
# ============================================================================
load_btn.on_click(load_files)
btn_summary.on_click(show_summary)
btn_heatmap.on_click(show_heatmap)
btn_snr.on_click(show_snr_graphs)
btn_skyplot.on_click(show_skyplot)
btn_anomaly.on_click(show_anomalies)
export_btn.on_click(export_report)
clear_btn.on_click(clear_all)


# ============================================================================
# LAYOUT
# ============================================================================
file_box = widgets.VBox([
    header,
    obs_section,
    widgets.HBox([obs_upload, obs_path_input]),
    nav_section,
    widgets.HBox([nav_upload, nav_path_input]),
    widgets.HBox([auto_download_nav, load_btn]),
])

config_box = widgets.VBox([
    config_section,
    widgets.HBox([preset_dropdown]),
    preset_help,
    elevation_slider,
    time_bin_slider,
    widgets.HBox([system_checks['G'], system_checks['R'], system_checks['E'], system_checks['C']]),
])

output_buttons = widgets.HBox([
    btn_summary, btn_heatmap, btn_snr, btn_skyplot, btn_anomaly
], layout=widgets.Layout(margin='10px 0'))

action_box = widgets.VBox([
    buttons_section,
    output_buttons,
    widgets.HBox([export_btn, clear_btn]),
    progress,
    status
])

main_widget = widgets.VBox([
    file_box,
    widgets.HTML("<hr>"),
    config_box,
    widgets.HTML("<hr>"),
    action_box,
    widgets.HTML("<hr>"),
    info_out,
    results_out
])

# Display the widget
display(main_widget)
print(f"✅ GeoVeil CN0 Widget v{LIB_VERSION} loaded")
print("   Features: Presets | Lock Integrity | Per-Satellite Graphs | Time-vs-Satellite Heatmap")
