# Post-quantum known-answer vectors

These are the first cases from pyca/cryptography's pinned vector files at
19ff77880bbd485464849366122982cd3a04d94b, under
vectors/cryptography_vectors/asymmetric/{MLDSA,MLKEM}/kat_*.rsp.
The upstream vectors are dual licensed Apache-2.0 OR BSD-3-Clause.
The ML-DSA mu field was independently calculated with Python hashlib SHAKE256
from the source public key, context, and message, following FIPS 204.
Only the fields needed for these tests are copied; the full upstream suites
exercise all cases.

The PKCS#12 and PKCS#7 fixtures are copied from the same pinned repository,
under vectors/cryptography_vectors/{pkcs12,pkcs7}. pkcs12-ca.der is its
pkcs12/ca/ca.pem certificate converted to DER. These exercise native container
ownership, password failure, optional outputs, and certificate byte identity.
