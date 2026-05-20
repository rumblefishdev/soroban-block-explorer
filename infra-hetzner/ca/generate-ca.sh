#!/usr/bin/env bash
#
# One-time bootstrap of the team-owned mTLS Certificate Authority.
#
# Generates an ECDSA P-256 CA key + self-signed certificate. The
# certificate is committed to the repo at infra-hetzner/ca/ca.crt;
# the private key is the operator's responsibility to transfer to
# the team password manager — this script never persists it to a
# stable filesystem.
#
# Usage:
#   ./generate-ca.sh
#
# Output:
#   infra-hetzner/ca/ca.crt          (public, will be committed)
#   /dev/shm/soroban-ca/ca.key        (private, ephemeral — move to password manager IMMEDIATELY)
#
# Idempotency:
#   This script refuses to overwrite an existing ca.crt or ca.key.
#   Re-running after a successful CA bootstrap is a no-op and prints
#   a clear error. Rotating the CA is a deliberate operation
#   documented in README.md and requires manual removal of the
#   existing artefacts first.

set -euo pipefail

# ---------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------

readonly SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly CA_CRT="${SCRIPT_DIR}/ca.crt"

# tmpfs is RAM-backed; ca.key never hits a persistent block device
# while this script runs. The operator copies it from here to the
# password manager and then deletes the tmpfs directory.
readonly KEY_DIR="/dev/shm/soroban-ca"
readonly CA_KEY="${KEY_DIR}/ca.key"

# CN includes the generation year so a future rotation produces
# a distinguishable Issuer in audit logs (old + new CAs would
# otherwise have identical Subjects, making post-rotation chain
# inspection ambiguous). The `O=` field anchors the CA to an
# organisation name independent of the per-generation CN.
readonly CA_ORG="Soroban"
readonly CA_CN="soroban-internal-ca-$(date -u +%Y)"
readonly CA_SUBJECT="/O=${CA_ORG}/CN=${CA_CN}"
readonly CA_DAYS=3650  # 10 years — long-lived root, rotation is a planned operation

# ---------------------------------------------------------------------
# Pre-flight
# ---------------------------------------------------------------------

if [[ ! -d /dev/shm ]]; then
    echo "ERROR: /dev/shm not available; refusing to write the CA key to a persistent disk." >&2
    echo "       This script requires a tmpfs to keep the private key off disk." >&2
    echo "       /dev/shm is Linux-specific. On macOS / BSD this CA tooling is" >&2
    echo "       NOT supported — run generate-ca.sh from a Linux laptop or VM," >&2
    echo "       then transfer the public ca.crt back via the repo and the" >&2
    echo "       private key via the team password manager. macOS support is" >&2
    echo "       out of scope (would need a secure ephemeral mount like a" >&2
    echo "       per-run encrypted disk image rather than tmpfs)." >&2
    exit 1
fi

if [[ -f "${CA_CRT}" ]]; then
    echo "ERROR: ${CA_CRT} already exists." >&2
    echo "       Re-running generate-ca.sh would orphan every client certificate already signed." >&2
    echo "       To rotate the CA: follow the procedure in infra-hetzner/ca/README.md." >&2
    exit 1
fi

if [[ -e "${CA_KEY}" ]]; then
    echo "ERROR: ${CA_KEY} already exists." >&2
    echo "       A previous run was interrupted before the operator moved the key to the password manager." >&2
    echo "       Inspect it; if it is the canonical CA key, move it to the password manager and remove from tmpfs." >&2
    exit 1
fi

if ! command -v openssl >/dev/null 2>&1; then
    echo "ERROR: openssl not found on PATH." >&2
    exit 1
fi

# openssl 1.1.1+ for -addext support; 3.x preferred.
readonly OPENSSL_VERSION="$(openssl version | awk '{print $2}')"
case "${OPENSSL_VERSION}" in
    1.1.*|3.*) : ;;
    *)
        echo "ERROR: openssl ${OPENSSL_VERSION} is too old; need 1.1.1 or later." >&2
        exit 1
        ;;
esac

# ---------------------------------------------------------------------
# Generation
# ---------------------------------------------------------------------

# tmpfs directory readable only by the invoking user.
install -d -m 0700 "${KEY_DIR}"

# Clean up on any non-success path: the key never lingers in tmpfs
# unintentionally. Successful runs print the path explicitly so the
# operator knows to move it. We do NOT delete on success — that is
# the operator's deliberate next step after the password-manager
# transfer.
cleanup_on_failure() {
    local rc=$?
    if [[ $rc -ne 0 ]]; then
        rm -f "${CA_KEY}"
        rmdir --ignore-fail-on-non-empty "${KEY_DIR}" 2>/dev/null || true
        rm -f "${CA_CRT}"
        echo "Failed run cleaned up; no half-written artefacts left behind." >&2
    fi
}
trap cleanup_on_failure EXIT

echo "==> Generating CA private key (ECDSA P-256) at ${CA_KEY}"
openssl genpkey \
    -algorithm EC \
    -pkeyopt ec_paramgen_curve:P-256 \
    -out "${CA_KEY}"
chmod 0600 "${CA_KEY}"

echo "==> Generating self-signed CA certificate at ${CA_CRT}"
openssl req \
    -new -x509 \
    -key "${CA_KEY}" \
    -out "${CA_CRT}" \
    -days "${CA_DAYS}" \
    -subj "${CA_SUBJECT}" \
    -addext "basicConstraints=critical,CA:TRUE,pathlen:0" \
    -addext "keyUsage=critical,keyCertSign,cRLSign" \
    -addext "subjectKeyIdentifier=hash"
chmod 0644 "${CA_CRT}"

# ---------------------------------------------------------------------
# Operator hand-off
# ---------------------------------------------------------------------

cat <<EOF

============================================================
CA bootstrap complete.

  Public certificate: ${CA_CRT}
  Private key (tmpfs): ${CA_KEY}

NEXT STEPS — do these BEFORE closing this shell:

  1. Copy the private key contents into a new password-manager
     entry named exactly:  soroban-prod / ca-key
       cat ${CA_KEY}

  2. Verify the entry by reading it back from the password
     manager and diff-ing against the tmpfs copy.

  3. Wipe the tmpfs copy:
       shred -u ${CA_KEY}
       rmdir ${KEY_DIR}

  4. Commit the public certificate:
       git add ${CA_CRT}
       git commit -m "chore(lore-0227): bootstrap mTLS CA public certificate"

The tmpfs key disappears on reboot, but DO NOT rely on that —
shred-then-rmdir is the canonical close.
============================================================
EOF
