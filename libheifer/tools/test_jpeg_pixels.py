#!/usr/bin/env python3
"""Focused native/Rust JPEG pixel and option modes for mutation sensitivity."""
import sys
import test_jpeg
if __name__=='__main__':
    if '--modes' not in sys.argv:sys.argv+=['--modes','0,3,11,27']
    if '--work' not in sys.argv:sys.argv+=['--work','.build/jpeg-pixels']
    if '--output' not in sys.argv:sys.argv+=['--output','.build/jpeg-pixels-report.json']
    test_jpeg.main()
