import json
import os
import sys
import tomllib
from pathlib import Path

tag = os.environ['GITHUB_REF_NAME']
with Path('package.json').open('rb') as manifest:
    npm_version = json.load(manifest)['version']
with Path('Cargo.toml').open('rb') as manifest:
    cargo_version = tomllib.load(manifest)['package']['version']

if tag != f'v{npm_version}' or tag != f'v{cargo_version}':
    print(
        f'::error::Tag {tag} does not match package.json version {npm_version} '
        f'and Cargo.toml version {cargo_version}.'
    )
    print('::error::Delete the tag, commit the correct version changes, and push the tag again.')
    sys.exit(1)

print(f'Tag {tag} matches both package versions.')
