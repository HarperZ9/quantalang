#!/usr/bin/env bash
# Verify No Secrets Hook — Stop

if ! git rev-parse --is-inside-work-tree &>/dev/null 2>&1; then exit 0; fi

STAGED=$(git diff --cached --name-only 2>/dev/null)
if [ -z "$STAGED" ]; then exit 0; fi

VIOLATIONS=""

SENSITIVE_BASENAMES=".env .env.local .env.production secrets.json credentials.json service-account.json .npmrc .pypirc"
for pattern in $SENSITIVE_BASENAMES; do
    while IFS= read -r file; do
        basename=$(basename "$file")
        if [ "$basename" = "$pattern" ]; then
            VIOLATIONS="${VIOLATIONS}\n  - SENSITIVE FILE STAGED: $file"
        fi
    done <<< "$STAGED"
done

while IFS= read -r file; do
    basename=$(basename "$file")
    case "$basename" in
        id_rsa|id_ed25519|id_ecdsa|id_dsa|*.pem|*.key)
            VIOLATIONS="${VIOLATIONS}\n  - PRIVATE KEY FILE STAGED: $file"
            ;;
    esac
done <<< "$STAGED"

while IFS= read -r file; do
    if [ -f "$file" ]; then
        if grep -qEi '(api[_-]?key|secret[_-]?key|password|token)\s*[:=]\s*["\x27][A-Za-z0-9+/=_-]{16,}' "$file" 2>/dev/null; then
            VIOLATIONS="${VIOLATIONS}\n  - POSSIBLE SECRET in $file"
        fi
        if grep -qE 'AKIA[0-9A-Z]{16}' "$file" 2>/dev/null; then
            VIOLATIONS="${VIOLATIONS}\n  - AWS ACCESS KEY in $file"
        fi
        if grep -qE '(ghp_[A-Za-z0-9]{36,}|gho_[A-Za-z0-9]{36,}|github_pat_[A-Za-z0-9_]{22,})' "$file" 2>/dev/null; then
            VIOLATIONS="${VIOLATIONS}\n  - GITHUB TOKEN in $file"
        fi
        if grep -qE '(sk_live_|pk_live_|rk_live_)[A-Za-z0-9]{20,}' "$file" 2>/dev/null; then
            VIOLATIONS="${VIOLATIONS}\n  - STRIPE KEY in $file"
        fi
        if grep -qE '-----BEGIN (RSA |EC |DSA |OPENSSH )?PRIVATE KEY-----' "$file" 2>/dev/null; then
            VIOLATIONS="${VIOLATIONS}\n  - PEM PRIVATE KEY in $file"
        fi
    fi
done <<< "$STAGED"

if [ -n "$VIOLATIONS" ]; then
    echo -e "POTENTIAL SECRETS DETECTED:${VIOLATIONS}" >&2
    echo "Review staged files before committing." >&2
    exit 2
fi

exit 0
