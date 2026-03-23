#!/usr/bin/env python
"""
Block Secrets Hook — PreToolUse
Prevents Claude from reading or editing sensitive files.
Exit code 2 = block operation and tell Claude why.
"""
import json
import sys
from pathlib import Path

SENSITIVE_FILENAMES = {
    '.env', '.env.local', '.env.production', '.env.staging', '.env.development',
    'secrets.json', 'secrets.yaml', 'id_rsa', 'id_ed25519',
    '.npmrc', '.pypirc', 'credentials.json', 'service-account.json',
    '.docker/config.json',
}

SENSITIVE_PATTERNS = ['aws/credentials', '.ssh/', 'private_key', 'secret_key']

try:
    data = json.load(sys.stdin)
    tool_name = data.get('tool_name', '')
    file_path = data.get('tool_input', {}).get('file_path', '')

    if not file_path:
        sys.exit(0)

    path = Path(file_path)

    if tool_name == 'Write' and path.name.startswith('.env'):
        sys.exit(0)

    if path.name in SENSITIVE_FILENAMES:
        print(f"BLOCKED: Access to '{file_path}' denied. This is a sensitive file.", file=sys.stderr)
        sys.exit(2)

    for pattern in SENSITIVE_PATTERNS:
        if pattern in str(path):
            print(f"BLOCKED: Access to '{file_path}' denied. Path matches sensitive pattern '{pattern}'.", file=sys.stderr)
            sys.exit(2)

    sys.exit(0)

except Exception as e:
    print(f"Hook error: {e}", file=sys.stderr)
    sys.exit(1)
