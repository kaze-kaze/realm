#!/usr/bin/env python3
"""Create the throwaway credentials of a test run: secrets.json and pki/.

  mksecrets.py [--out DIR] [--force]

Run it once before packing the lab for the test hosts. Nothing it writes is kept in
the repository (see .gitignore): every run of the test bench gets its own passwords,
keys and certificates, and they are deleted with the bench afterwards.

The certificates need OpenSSL 1.1 or newer. LibreSSL (the macOS default) may write EC
keys with explicit curve parameters, which Go programs (Hysteria, Xray) refuse; the
script checks for that and stops. Running it inside the rbt container always works.
"""
import argparse
import base64
import json
import os
import secrets
import subprocess
import sys
import tempfile
import uuid

LAB = os.path.dirname(os.path.abspath(__file__))
TLS_NAME = "rbt.test"
WG_CLIENTS = 48
DAYS = "60"


def b64(n):
    return base64.b64encode(secrets.token_bytes(n)).decode()


def make_secrets():
    return {
        "uuid": str(uuid.uuid4()),
        "trojan_pw": secrets.token_hex(12),
        "ss_pw": secrets.token_hex(16),
        "ss2022_key": b64(16),
        "hy_pw": secrets.token_hex(12),
        "hy_obfs": secrets.token_hex(12),
        # x25519 private key the way xray prints it: url-safe base64 without padding
        "reality_priv_raw": base64.urlsafe_b64encode(secrets.token_bytes(32)).decode().rstrip("="),
        "reality_sid": secrets.token_hex(8),
        "wg_server": b64(32),
        "wg_clients": [b64(32) for _ in range(WG_CLIENTS)],
    }


def write_private(path, data):
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(fd, "w") as f:
        f.write(data)


def run(*cmd):
    r = subprocess.run(cmd, capture_output=True, text=True)
    if r.returncode != 0:
        raise SystemExit("%s failed: %s" % (" ".join(cmd[:3]), r.stderr.strip()))
    return r.stdout


def ec_key(path):
    run("openssl", "genpkey", "-algorithm", "EC", "-pkeyopt", "ec_paramgen_curve:P-256",
        "-pkeyopt", "ec_param_enc:named_curve", "-out", path)
    os.chmod(path, 0o600)
    if "ASN1 OID: prime256v1" not in run("openssl", "pkey", "-in", path, "-noout", "-text"):
        os.remove(path)
        raise SystemExit("this openssl wrote an EC key with explicit parameters; run mksecrets.py inside the rbt container")


def make_pki(d):
    os.makedirs(d, exist_ok=True)
    ca_key, ca_crt = os.path.join(d, "ca.key"), os.path.join(d, "ca.crt")
    key, crt = os.path.join(d, "leaf.key"), os.path.join(d, "leaf.crt")
    ec_key(ca_key)
    run("openssl", "req", "-x509", "-new", "-key", ca_key, "-subj", "/CN=rbt-test-ca", "-days", DAYS,
        "-addext", "basicConstraints=critical,CA:TRUE", "-out", ca_crt)
    ec_key(key)
    with tempfile.TemporaryDirectory() as tmp:
        csr, ext = os.path.join(tmp, "leaf.csr"), os.path.join(tmp, "leaf.ext")
        run("openssl", "req", "-new", "-key", key, "-subj", "/CN=" + TLS_NAME, "-out", csr)
        with open(ext, "w") as f:
            f.write("subjectAltName=DNS:%s\nbasicConstraints=CA:FALSE\nextendedKeyUsage=serverAuth\n" % TLS_NAME)
        run("openssl", "x509", "-req", "-in", csr, "-CA", ca_crt, "-CAkey", ca_key, "-set_serial",
            str(secrets.randbits(63)), "-days", DAYS, "-extfile", ext, "-out", crt)
    run("openssl", "verify", "-CAfile", ca_crt, crt)


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--out", default=LAB, help="directory to write to (default: the lab directory)")
    ap.add_argument("--force", action="store_true", help="replace credentials that already exist")
    a = ap.parse_args()
    sec, pki = os.path.join(a.out, "secrets.json"), os.path.join(a.out, "pki")
    if not a.force and (os.path.exists(sec) or os.path.exists(os.path.join(pki, "leaf.key"))):
        raise SystemExit("credentials already exist in %s (use --force to replace them)" % a.out)
    os.makedirs(a.out, exist_ok=True)
    make_pki(pki)
    write_private(sec, json.dumps(make_secrets(), indent=1) + "\n")
    print("wrote %s and %s/{ca,leaf}.{key,crt}" % (sec, pki))


if __name__ == "__main__":
    sys.exit(main())
