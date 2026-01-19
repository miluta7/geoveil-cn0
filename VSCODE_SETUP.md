# GeoVeil CN0 - VSCode Development Guide

## Quick Start (Windows)

### 1. Prerequisites
```powershell
# Install Rust (download installer from https://rustup.rs)
winget install Rustlang.Rustup

# Verify Python 3.8+
python --version

# Install maturin (Rust-Python bridge)
pip install maturin
```

### 2. Build the Library
```powershell
cd geoveil-cn0

# Development build (fast, debug)
maturin develop --features python

# OR Release build (optimized, use this for real analysis)
maturin develop --features python --release
```

### 3. Test Installation
```powershell
python -c "import geoveil_cn0 as g; print(f'Version: {g.VERSION}')"
```

### 4. Run the GUI
```powershell
pip install plotly pandas numpy
python geoveil_cn0_gui.py
```

---

## VSCode Setup

### Required Extensions
Install from Extensions panel (Ctrl+Shift+X):
- **rust-analyzer** - Rust language support
- **Python** (Microsoft) - Python support  
- **Even Better TOML** - Cargo.toml syntax highlighting

### Create `.vscode/settings.json`
```json
{
    "rust-analyzer.cargo.features": ["python"],
    "rust-analyzer.check.command": "clippy",
    "python.defaultInterpreterPath": "${workspaceFolder}/.venv/Scripts/python.exe",
    "editor.formatOnSave": true,
    "[rust]": {
        "editor.defaultFormatter": "rust-lang.rust-analyzer"
    }
}
```

### Create `.vscode/tasks.json` (Build Tasks)
```json
{
    "version": "2.0.0",
    "tasks": [
        {
            "label": "Build (Debug)",
            "type": "shell",
            "command": "maturin develop --features python",
            "group": "build",
            "problemMatcher": ["$rustc"],
            "presentation": {"reveal": "always"}
        },
        {
            "label": "Build (Release)",
            "type": "shell", 
            "command": "maturin develop --features python --release",
            "group": {"kind": "build", "isDefault": true},
            "problemMatcher": ["$rustc"],
            "presentation": {"reveal": "always"}
        },
        {
            "label": "Run Tests",
            "type": "shell",
            "command": "cargo test && python -c \"import geoveil_cn0; print('OK:', geoveil_cn0.VERSION)\"",
            "group": "test"
        },
        {
            "label": "Run GUI",
            "type": "shell",
            "command": "python geoveil_cn0_gui.py",
            "group": "none"
        },
        {
            "label": "Build Wheel",
            "type": "shell",
            "command": "maturin build --features python --release",
            "group": "build"
        }
    ]
}
```

### Keyboard Shortcuts
- **Ctrl+Shift+B** - Build (Release)
- **Ctrl+Shift+P** → "Tasks: Run Task" → select task

---

## Troubleshooting

### AppLocker Error (Windows)
```
An Application Control policy has blocked this file
```
**Solutions:**
1. Run VSCode/PowerShell as **Administrator**
2. Move project to `C:\Users\YourName\` instead of restricted directories
3. Use WSL (Windows Subsystem for Linux)

### Rust Not Found
```powershell
# Add Rust to PATH
$env:PATH += ";$env:USERPROFILE\.cargo\bin"
# Or restart terminal/VSCode after installing Rust
```

### Python Module Not Found
```powershell
# Make sure you're using the right Python
where python
pip show geoveil_cn0

# Rebuild if needed
maturin develop --features python --release
```

### Compilation Errors
```powershell
# Clean build
cargo clean
maturin develop --features python --release
```

---

## Project Structure

```
geoveil-cn0/
├── Cargo.toml           # Rust dependencies and config
├── pyproject.toml       # Python package config  
├── src/
│   ├── lib.rs           # Library entry point
│   ├── python.rs        # PyO3 Python bindings
│   ├── rinex.rs         # RINEX v2/v3/v4 parser
│   ├── cn0.rs           # CN0 analysis engine
│   ├── navigation.rs    # BRDC ephemeris computation
│   ├── visibility.rs    # Satellite visibility prediction
│   ├── tle.rs           # TLE/SGP4 fallback
│   └── types.rs         # Data structures
├── geoveil_cn0_gui.py   # Standalone GUI application
└── VSCODE_SETUP.md      # This file
```

---

## Build Commands Reference

| Command | Purpose |
|---------|---------|
| `maturin develop --features python` | Fast debug build |
| `maturin develop --features python --release` | Optimized release build |
| `maturin build --features python --release` | Build wheel for distribution |
| `cargo test` | Run Rust unit tests |
| `cargo clippy` | Lint Rust code |
| `cargo doc --open` | Generate and view documentation |

---

## API Quick Reference

```python
import geoveil_cn0 as gcn0

# Check version
print(gcn0.VERSION)

# Create analyzer with config
config = gcn0.AnalysisConfig(
    min_elevation=5.0,      # degrees
    time_bin=60,            # seconds
    detect_anomalies=True,
    anomaly_sensitivity=0.3,
    interference_threshold_db=8.0,
    verbose=True,
    nav_file=None,          # optional navigation file path
)

analyzer = gcn0.CN0Analyzer(config)

# Analyze RINEX file
result = analyzer.analyze_file("observation.rnx")

# Or with navigation file
result = analyzer.analyze_with_nav("observation.rnx", "navigation.rnx")

# Access results
print(f"Quality: {result.quality_score.overall}/100")
print(f"Rating: {result.quality_score.rating}")
print(f"CN0: {result.avg_cn0:.1f} dB-Hz")
print(f"Jamming: {result.jamming_detected}")
print(f"Spoofing: {result.spoofing_detected}")

# Get detailed data
anomalies = result.get_anomalies()
timeseries = result.get_timeseries_data()
skyplot = result.get_skyplot_data()
constellation_summary = result.get_constellation_summary("GPS")

# Export to JSON
json_str = result.to_json()
```
