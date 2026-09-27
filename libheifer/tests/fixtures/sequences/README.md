# Sequence reader fixtures

Generated with the untouched-header `tests/sequences.c` client and pinned native
libheif 1.23.4, commit 4e14f5942c1732ace9611b9522cc991501445463.
The cases are metadata: 0-1-3-90000-1000-1-0-40; timestamp:
0-1-3-90000-1000-1-65544-40; visual: 1-1-3-1000-1000-1-278528-40.
Stored as hex for review. In the two metadata fixtures only, the uninitialized
urim data_reference_index is set to zero when materializing the reader input.
That field is never used as writer parity evidence. All visual bytes are exact.
