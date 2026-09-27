#!/usr/bin/env python3
"""Focused original-header JPEG2000 sample and conversion mutation comparisons."""
import sys
import test_jpeg2000
if __name__=='__main__':
    if '--modes' not in sys.argv:sys.argv+=['--modes','0,3,11,27']
    test_jpeg2000.main()
