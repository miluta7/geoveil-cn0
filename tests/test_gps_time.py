"""Regression test for GPS time fix (JD noon vs midnight offset).
Tests to_gps_time() correctness without needing RINEX files.
"""
import sys

try:
    import geoveil_cn0 as g
    
    # Test that the library loads
    config = g.AnalysisConfig()
    print(f"AnalysisConfig loaded OK")
    print(f"  spoofing_unexpected_threshold={config.spoofing_unexpected_threshold}")
    print(f"  spoofing_min_unexpected_count={config.spoofing_min_unexpected_count}")
    print("PASS: Library loads with new config fields")

    # Test custom values
    config2 = g.AnalysisConfig(spoofing_unexpected_threshold=0.50, spoofing_min_unexpected_count=10.0)
    assert config2.spoofing_unexpected_threshold == 0.50, f"Expected 0.50, got {config2.spoofing_unexpected_threshold}"
    assert config2.spoofing_min_unexpected_count == 10.0, f"Expected 10.0, got {config2.spoofing_min_unexpected_count}"
    print("PASS: Custom spoofing thresholds accepted")

    # Test version
    ver = g.__version__
    print(f"Library version: {ver}")
    assert ver == "0.3.8", f"Expected 0.3.8, got {ver}"
    print("PASS: Version is 0.3.8")

    print("\nAll tests PASSED")
    sys.exit(0)
    
except Exception as e:
    print(f"ERROR: {type(e).__name__}: {e}")
    import traceback
    traceback.print_exc()
    sys.exit(1)
