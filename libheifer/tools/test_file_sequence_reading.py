#!/usr/bin/env python3
"""Independent native sequence fixtures read through file input."""
import test_sequence_reading
if __name__=='__main__':
    test_sequence_reading.CLIENT='tests/file_sequence_reading.c'
    test_sequence_reading.__doc__=__doc__
    test_sequence_reading.main()
