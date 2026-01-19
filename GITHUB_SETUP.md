# GitHub Setup Instructions for geoveil-cn0

## Quick Setup (5 minutes)

### Step 1: Create Repository on GitHub

1. Go to https://github.com/new
2. Repository name: `geoveil-cn0`
3. Description: `High-performance GNSS CN0 Signal Quality Analysis Library (Rust/Python)`
4. Make it **Public**
5. **Don't** initialize with README (we have one)
6. Click **Create repository**

### Step 2: Push Local Files

Run these commands in your terminal on odin:

```bash
# Navigate to your project directory
cd ~/geoveil-cn0

# Initialize git repository (if not already done)
git init

# Add all files
git add .

# Create initial commit
git commit -m "Initial release v0.3.1 - Published to PyPI"

# Add GitHub remote
git remote add origin https://github.com/miluta7/geoveil-cn0.git

# Push to GitHub
git branch -M main
git push -u origin main
```

### Step 3: Create Release Tag

```bash
# Create version tag
git tag -a v0.3.1 -m "Release v0.3.1 - First PyPI release"

# Push tag
git push origin v0.3.1
```

### Step 4: Create GitHub Release (Optional but Recommended)

1. Go to https://github.com/miluta7/geoveil-cn0/releases
2. Click **Draft a new release**
3. Choose tag: `v0.3.1`
4. Release title: `v0.3.1 - Initial Release`
5. Description:
   ```markdown
   ## First Public Release 🎉
   
   **geoveil-cn0** is now available on PyPI!
   
   ### Installation
   ```bash
   pip install geoveil-cn0
   ```
   
   ### Features
   - 🛰️ Multi-GNSS Support (GPS, GLONASS, Galileo, BeiDou)
   - 📊 CN0 Quality Scoring
   - 🚨 Jamming/Spoofing/Interference Detection
   - 📈 Anomaly Detection
   - 🗺️ Skyplot Visualization
   - ⚡ High Performance (Rust core)
   
   ### Links
   - PyPI: https://pypi.org/project/geoveil-cn0/
   ```
6. Click **Publish release**

---

## Set Up Automated PyPI Publishing (Optional)

### Add PyPI Token to GitHub Secrets

1. Go to https://github.com/miluta7/geoveil-cn0/settings/secrets/actions
2. Click **New repository secret**
3. Name: `PYPI_API_TOKEN`
4. Value: Your PyPI API token (starts with `pypi-`)
5. Click **Add secret**

Now, whenever you push a tag like `v0.3.2`, GitHub Actions will automatically:
1. Run tests
2. Build wheels for Linux, Windows, macOS
3. Publish to PyPI

---

## Repository Structure

```
geoveil-cn0/
├── .github/
│   └── workflows/
│       └── ci.yml          # GitHub Actions CI/CD
├── notebooks/
│   ├── geoveil-cn0-widget.ipynb
│   └── GeoVeil_CN0_Library_Documentation.ipynb
├── src/
│   ├── lib.rs              # Main library entry
│   ├── cn0.rs              # CN0 analysis core
│   ├── python.rs           # PyO3 bindings
│   ├── rinex.rs            # RINEX parser
│   ├── navigation.rs       # Nav file parser
│   ├── visibility.rs       # Satellite visibility
│   ├── tle.rs              # TLE/SGP4 support
│   └── types.rs            # Data structures
├── geoveil_cn0_gui.py      # Jupyter widget
├── Cargo.toml              # Rust configuration
├── pyproject.toml          # Python packaging
├── README.md
├── LICENSE
└── .gitignore
```

---

## Future Releases

When you want to release a new version:

```bash
# 1. Update version in all files
# - Cargo.toml: version = "0.3.2"
# - pyproject.toml: version = "0.3.2"  
# - src/lib.rs: pub const VERSION: &str = "0.3.2";

# 2. Commit changes
git add .
git commit -m "Bump version to 0.3.2"

# 3. Create and push tag
git tag -a v0.3.2 -m "Release v0.3.2"
git push origin main
git push origin v0.3.2

# 4. GitHub Actions will automatically publish to PyPI
# (if PYPI_API_TOKEN secret is configured)
```

---

## Troubleshooting

### Authentication Error

If you get authentication errors when pushing:

```bash
# Use GitHub token instead of password
git remote set-url origin https://YOUR_TOKEN@github.com/miluta7/geoveil-cn0.git
```

Or use SSH:
```bash
git remote set-url origin git@github.com:miluta7/geoveil-cn0.git
```

### Large File Errors

If you accidentally committed large files:
```bash
# Remove target directory from history
git filter-branch --force --index-filter \
  'git rm -rf --cached --ignore-unmatch target/' HEAD
```

---

## Links

- **PyPI**: https://pypi.org/project/geoveil-cn0/
- **GitHub**: https://github.com/miluta7/geoveil-cn0
- **Author**: Miluta Dulea-Flueras
